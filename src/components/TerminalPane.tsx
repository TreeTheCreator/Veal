// One xterm.js terminal bound to one PTY, with the pane chrome from the
// mockup: status badge, project name, cwd, close. Positioned absolutely by
// PaneLayout so it never remounts when the split tree changes shape.
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { spawnPty, type PtySession } from "../pty";
import { createResizeScheduler } from "../resize";
import type { PaneNode } from "../layout/tree";
import { basename, tilde } from "../paths";
import { useLayout } from "../layout/store";
import { projectForCwd, useProjects } from "../projects/store";
import { proxyForSpawn, shellForSpawn, useSettings } from "../settings/store";
import { terminalTheme, useEffectiveMode } from "../theme";
import { terminalKeyAction } from "../ui";

// Fallback when the container has no layout yet at first fit().
const DEFAULT_COLS = 120;
const DEFAULT_ROWS = 30;

const isWindows = navigator.userAgent.includes("Windows");

interface Props {
  pane: PaneNode;
  /** Detected agent CLI running under this pane's shell (plan-08), or null. */
  agent: string | null;
  focused: boolean;
  style: CSSProperties;
  onFocus(): void;
  onClose(): void;
}

export function TerminalPane({ pane, agent, focused, style, onFocus, onClose }: Props) {
  const { id, cwd } = pane;
  // Colour comes from detection alone (plan-17): red and named while an
  // agent runs, blue `shell` otherwise.
  const accent = agent ? "agent" : "shell";
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const [exitCode, setExitCode] = useState<number | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  // Sidebar project name for the header; basename when the pane is elsewhere.
  const projectName = useProjects((s) => projectForCwd(s.projects, cwd)?.name ?? basename(cwd));
  const mode = useEffectiveMode();

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      fontSize: 13,
      fontFamily: '"Geist Mono", "Cascadia Mono", Consolas, monospace',
      scrollback: 5000,
      cursorBlink: true,
      allowProposedApi: true,
      theme: terminalTheme(),
      // Turns on xterm's ConPTY heuristics (scrollback on resize, no reflow).
      windowsPty: isWindows ? { backend: "conpty", buildNumber: 26200 } : undefined,
    });
    termRef.current = term;
    // Let Ctrl+1/2/3 reach the window listener (view switching) instead of
    // the shell, Ctrl+C over a selection copy it rather than send ^C
    // (plan-16), and Ctrl+V paste rather than send ^V. Returning false keeps
    // the key from xterm entirely.
    term.attachCustomKeyEventHandler((e) => {
      switch (terminalKeyAction(e, term.hasSelection())) {
        case "view":
          return false;
        case "copy":
          // xterm asks on keydown, keypress and keyup; copy once.
          if (e.type === "keydown") {
            navigator.clipboard.writeText(term.getSelection()).catch((err) => console.warn("clipboard", err));
            term.clearSelection();
            e.preventDefault();
          }
          return false;
        case "paste":
          // Keep it from xterm but let the browser's default paste run: the
          // textarea's `paste` event is xterm's paste path (bracketed paste).
          return false;
        default:
          return true;
      }
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);

    // GPU renderer; fall back to the DOM renderer if the context is lost.
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      term.loadAddon(webgl);
    } catch (e) {
      console.warn("WebGL renderer unavailable, using DOM renderer", e);
    }

    fit.fit();
    const cols = term.cols || DEFAULT_COLS;
    const rows = term.rows || DEFAULT_ROWS;

    let session: PtySession | null = null;
    let disposed = false;
    const disposables: Array<() => void> = [];

    // Wire input before spawning. ConPTY's conhost opens with a cursor-position
    // query (ESC [ 6 n) and stalls all output until the terminal answers it;
    // when several panes spawn at once (layout restore) that query can reach
    // xterm before pty_spawn resolves, so replies are queued until it does.
    const pendingInput: string[] = [];
    disposables.push(
      term.onData((d) => {
        if (session) session.write(d).catch((e) => console.warn("pty_write", e));
        else pendingInput.push(d);
      }).dispose,
    );

    // Dev-only hook so panes can be driven and inspected over CDP.
    if (import.meta.env.DEV) {
      const w = window as unknown as { __veal?: { panes: Map<number, unknown> } };
      w.__veal ??= { panes: new Map() };
      w.__veal.panes.set(id, {
        term,
        get session() {
          return session;
        },
      });
      disposables.push(() => w.__veal?.panes.delete(id));
    }

    const resizer = createResizeScheduler(({ cols, rows }) => {
      session?.resize(cols, rows).catch((e) => console.warn("pty_resize", e));
    });
    const observer = new ResizeObserver(() => {
      if (disposed) return;
      fit.fit();
      resizer.schedule({ cols: term.cols, rows: term.rows });
    });
    observer.observe(host);

    // The shell and proxy settings are read at spawn time: they apply to new panes only.
    const shell = shellForSpawn(useSettings.getState().shell);
    const proxy = proxyForSpawn(useSettings.getState().proxyPort);
    // A spawn is the one thing that notices a pinned root going away or
    // coming back (plan-16); the sidebar shows it.
    const markRoot = (missing: boolean) => {
      const { projects, markMissing } = useProjects.getState();
      const project = projectForCwd(projects, cwd);
      if (project) markMissing(project.id, missing);
    };
    // xterm's write callback fires once the bytes are parsed; the next
    // fetch waits for it (plan-15).
    spawnPty({ cwd, cols, rows, shell, proxy }, (bytes, done) => term.write(bytes, done))
      .then((s) => {
        if (disposed) {
          s.kill().catch(() => {});
          return;
        }
        session = s;
        markRoot(false);
        for (const d of pendingInput.splice(0)) s.write(d).catch((e) => console.warn("pty_write", e));
        // The pane may have been resized while spawn was in flight.
        if (term.cols !== cols || term.rows !== rows) {
          resizer.schedule({ cols: term.cols, rows: term.rows });
        }
        disposables.push(
          s.onExit((code) => {
            // A clean `exit` closes the pane; anything else stays visible.
            // Straight to the store: the shell is gone, so the agent close
            // guard on `onClose` (plan-16) must not ask.
            if (code === 0) useLayout.getState().closePane(id);
            else setExitCode(code);
          }),
          // Detection is keyed by PTY id in Rust; the layout keys by pane id.
          s.onAgent((agent) => useLayout.getState().setAgent(id, agent)),
        );
      })
      .catch((e) => {
        setError(String(e));
        if (String(e).startsWith("no such directory")) markRoot(true);
      });

    return () => {
      disposed = true;
      observer.disconnect();
      resizer.cancel();
      disposables.forEach((d) => d());
      session?.kill().catch(() => {});
      term.dispose();
      termRef.current = null;
    };
  }, [id, cwd]);

  // Plan-11: xterm's palette follows the app's tokens. Declared after the
  // mount effect so the first run sees the terminal; re-runs on mode change.
  useEffect(() => {
    const term = termRef.current;
    if (term) term.options.theme = terminalTheme();
  }, [mode]);

  useEffect(() => {
    if (focused) termRef.current?.focus();
  }, [focused]);

  return (
    <div
      className={`pane ${accent}${focused ? " focused" : ""}`}
      style={style}
      data-pane={id}
      onPointerDown={onFocus}
    >
      <div className="pane-head">
        <span className={`badge ${accent}`} title={agent ? `${agent} running` : undefined}>
          {agent ?? "shell"}
        </span>
        <span className="pname">{projectName}</span>
        <span className="cwd" title={cwd}>{tilde(cwd)}</span>
        <button
          className="close"
          title="Close pane"
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
        >
          ×
        </button>
      </div>
      <div className="pane-term" ref={hostRef} />
      {error && <div className="pane-footer error">failed to start shell: {error}</div>}
      {exitCode !== undefined && (
        <div className="pane-footer">process exited (code {exitCode ?? "?"})</div>
      )}
    </div>
  );
}
