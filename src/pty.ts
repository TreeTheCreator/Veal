// Thin wrapper over the pty_* commands so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProxyMode } from "./settings/store";

export interface SpawnOptions {
  cwd: string;
  cols: number;
  rows: number;
  shell?: string;
  /** Plan-18: Rust resolves the proxy from these at spawn time. */
  proxyMode?: ProxyMode;
  proxyPort?: string;
  /** `no_proxy`, comma separated; omitted means Rust's default. */
  proxyBypass?: string;
}

/** What `proxy_resolve` answers: exactly what a new pane would get (plan-18). */
export interface ProxyResolved {
  source: ProxyMode;
  /** `http://host:port`, or `socks5h://host:port` when `socksOnly`; null exports nothing. */
  url: string | null;
  socksOnly: boolean;
  /** Windows' bypass entries that are added to `no_proxy`. */
  bypass: string[];
  notes: string[];
  /** The exact variables a new pane gets, in order. */
  env: Array<[string, string]>;
  diag: {
    lookup: "static" | "script" | "autodetect" | "none" | "unavailable" | null;
    rawProxy: string | null;
    rawBypass: string | null;
    scriptUrl: string | null;
    host: string | null;
    answered: string | null;
    lookupMs: number | null;
    errorCode: number | null;
  };
}

type ProxyOptions = Pick<SpawnOptions, "proxyMode" | "proxyPort" | "proxyBypass">;

/** The argument object both proxy commands take; keys must match Rust's. */
function proxyArgs(opts: ProxyOptions) {
  return { proxyMode: opts.proxyMode ?? null, proxyPort: opts.proxyPort ?? null, proxyBypass: opts.proxyBypass ?? null };
}

export function resolveProxy(opts: ProxyOptions): Promise<ProxyResolved> {
  return invoke<ProxyResolved>("proxy_resolve", proxyArgs(opts));
}

/**
 * Output callback (plan-15): `done` must be called once xterm has consumed
 * `bytes`; the next fetch waits for it, which is what gives the whole chain,
 * down to the child's write(), backpressure.
 */
export type OnData = (bytes: Uint8Array, done: () => void) => void;

export interface PtySession {
  id: number;
  write(data: string): Promise<void>;
  resize(cols: number, rows: number): Promise<void>;
  kill(): Promise<void>;
  /** Fires once when the child exits. Returns an unsubscribe function. */
  onExit(cb: (code: number | null) => void): () => void;
  /**
   * Fires when the agent CLI detected under this shell changes (plan-08):
   * its name, or null once it exits. Returns an unsubscribe function.
   */
  onAgent(cb: (agent: string | null) => void): () => void;
}

interface PtyExit {
  id: number;
  code: number | null;
}

interface PtyData {
  id: number;
}

// Output is pulled (plan-15): Rust emits `pty-data` once when a pane's outbox
// goes from empty to non-empty, and the session drains it with `pty_read`
// until empty. One app-wide listener, fanned out by PTY id to the drain.
const dataHandlers = new Map<number, () => void>();
let dataListener: Promise<UnlistenFn> | null = null;

function ensureDataListener() {
  dataListener ??= listen<PtyData>("pty-data", (ev) => dataHandlers.get(ev.payload.id)?.());
}

/** Mirrors `AGENTS` in src-tauri/src/agents.rs — display only; Rust decides. */
export const AGENTS = ["claude", "codex", "aider", "gemini", "copilot", "amp", "goose", "opencode"];

interface PaneAgent {
  id: number;
  agent: string | null;
}

// One app-wide listener for `pty-agent`, fanned out by PTY id. The PTY id is
// not the pane id: the pane that owns the session maps it (TerminalPane).
const agentHandlers = new Map<number, Set<(agent: string | null) => void>>();
let agentListener: Promise<UnlistenFn> | null = null;

function ensureAgentListener() {
  agentListener ??= listen<PaneAgent>("pty-agent", (ev) => {
    const { id, agent } = ev.payload;
    agentHandlers.get(id)?.forEach((cb) => cb(agent));
  });
}

// One app-wide listener for `pty-exit`, fanned out by pane id. An exit
// with no handler yet is remembered (plan-13): a shell that dies at once
// can emit before `pty_spawn`'s reply lets the pane register one.
const exitHandlers = new Map<number, Set<(code: number | null) => void>>();
const earlyExits = new Map<number, number | null>();
let exitListener: Promise<UnlistenFn> | null = null;

function ensureExitListener() {
  exitListener ??= listen<PtyExit>("pty-exit", (ev) => {
    const { id, code } = ev.payload;
    const handlers = exitHandlers.get(id);
    exitHandlers.delete(id);
    // Nothing more comes for this pid (plan-14); the pane's own unsubscribe is null-safe.
    agentHandlers.delete(id);
    if (handlers) handlers.forEach((cb) => cb(code));
    else earlyExits.set(id, code);
  });
}

/**
 * Kill every shell a previous page left behind (plan-16). A page reload
 * skips React cleanup, so a boot that finds panes registered is a page
 * whose predecessor is gone; their late exits and drains are for nobody.
 */
export async function resetSessions(): Promise<void> {
  await invoke("pty_reset");
  earlyExits.clear();
  dataHandlers.clear();
}

export async function spawnPty(opts: SpawnOptions, onData: OnData): Promise<PtySession> {
  ensureExitListener();
  ensureAgentListener();
  ensureDataListener();
  const id = await invoke<number>("pty_spawn", {
    cwd: opts.cwd,
    cols: opts.cols,
    rows: opts.rows,
    shell: opts.shell ?? null,
    ...proxyArgs(opts),
  });

  // The drain: one loop per session, fetching until the outbox is empty. A
  // signal during a loop asks for one more fetch rather than a second loop;
  // a signal outside one starts it. `pty_read` answers with an ArrayBuffer.
  let live = true;
  let running = false;
  let again = false;
  const drain = async () => {
    if (running) {
      again = true;
      return;
    }
    running = true;
    try {
      while (live) {
        again = false;
        const bytes = new Uint8Array(await invoke<ArrayBuffer>("pty_read", { id }));
        if (!live) break;
        if (bytes.length === 0) {
          if (again) continue;
          break;
        }
        await new Promise<void>((done) => onData(bytes, done));
      }
    } catch (e) {
      console.warn("pty_read", e);
    } finally {
      running = false;
    }
  };
  dataHandlers.set(id, drain);
  // The shell may have written (and signalled) before `pty_spawn` replied:
  // that signal is folded into Rust's `notified` flag, so one unconditional
  // drain covers it and nothing before this point can be lost.
  void drain();

  return {
    id,
    write: (data) => invoke("pty_write", { id, data }),
    resize: (cols, rows) => invoke("pty_resize", { id, cols, rows }),
    kill: () => {
      // Rust drops the outbox on kill; stop fetching and stop listening.
      live = false;
      dataHandlers.delete(id);
      return invoke("pty_kill", { id });
    },
    onExit(cb) {
      if (earlyExits.has(id)) {
        const code = earlyExits.get(id)!;
        earlyExits.delete(id);
        let live = true;
        queueMicrotask(() => live && cb(code));
        return () => {
          live = false;
        };
      }
      let set = exitHandlers.get(id);
      if (!set) exitHandlers.set(id, (set = new Set()));
      set.add(cb);
      return () => {
        exitHandlers.get(id)?.delete(cb);
      };
    },
    onAgent(cb) {
      let set = agentHandlers.get(id);
      if (!set) agentHandlers.set(id, (set = new Set()));
      set.add(cb);
      return () => {
        agentHandlers.get(id)?.delete(cb);
        if (agentHandlers.get(id)?.size === 0) agentHandlers.delete(id);
      };
    },
  };
}
