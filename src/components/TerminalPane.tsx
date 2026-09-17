// One xterm.js terminal bound to one PTY. Step 2 scope: a single pane filling
// the content area. Pane chrome (header, badges, splits) arrives in step 3.
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { spawnPty, type PtySession } from "../pty";
import { createResizeScheduler } from "../resize";

// Fallback when the container has no layout yet at first fit().
const DEFAULT_COLS = 120;
const DEFAULT_ROWS = 30;

const isWindows = navigator.userAgent.includes("Windows");

interface Props {
  cwd: string;
}

export function TerminalPane({ cwd }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [exitCode, setExitCode] = useState<number | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      fontSize: 13,
      fontFamily: '"Geist Mono", "Cascadia Mono", Consolas, monospace',
      scrollback: 5000,
      cursorBlink: true,
      allowProposedApi: true,
      theme: {
        background: "#0c0c0d",
        foreground: "#ececef",
        cursor: "#7aa2ff",
        selectionBackground: "rgba(255,255,255,0.07)",
      },
      // Turns on xterm's ConPTY heuristics (scrollback on resize, no reflow).
      windowsPty: isWindows ? { backend: "conpty", buildNumber: 26200 } : undefined,
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

    // Dev-only hook so the pane can be driven and inspected over CDP.
    if (import.meta.env.DEV) {
      (window as unknown as { __veal?: unknown }).__veal = {
        term,
        get session() {
          return session;
        },
      };
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

    spawnPty({ cwd, cols, rows }, (bytes) => term.write(bytes))
      .then((s) => {
        if (disposed) {
          s.kill().catch(() => {});
          return;
        }
        session = s;
        // The pane may have been resized while spawn was in flight.
        if (term.cols !== cols || term.rows !== rows) {
          resizer.schedule({ cols: term.cols, rows: term.rows });
        }
        disposables.push(term.onData((d) => s.write(d).catch((e) => console.warn("pty_write", e))).dispose);
        disposables.push(s.onExit((code) => setExitCode(code)));
        term.focus();
      })
      .catch((e) => setError(String(e)));

    return () => {
      disposed = true;
      observer.disconnect();
      resizer.cancel();
      disposables.forEach((d) => d());
      session?.kill().catch(() => {});
      term.dispose();
    };
  }, [cwd]);

  return (
    <div className="pane">
      <div className="pane-term" ref={hostRef} />
      {error && <div className="pane-footer error">failed to start shell: {error}</div>}
      {exitCode !== undefined && (
        <div className="pane-footer">process exited (code {exitCode ?? "?"})</div>
      )}
    </div>
  );
}
