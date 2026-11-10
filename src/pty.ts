// Thin wrapper over the pty_* commands so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SessionInfo } from "./layout/sessions";
import type { ProxyMode } from "./settings/store";

export interface OpenOptions {
  /** The pane's session key: attaches if that shell is running, else spawns it. */
  key: string;
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

type ProxyOptions = Pick<OpenOptions, "proxyMode" | "proxyPort" | "proxyBypass">;

/** The argument object both proxy commands take; keys must match Rust's. */
function proxyArgs(opts: ProxyOptions) {
  return { proxyMode: opts.proxyMode ?? null, proxyPort: opts.proxyPort ?? null, proxyBypass: opts.proxyBypass ?? null };
}

export function resolveProxy(opts: ProxyOptions): Promise<ProxyResolved> {
  return invoke<ProxyResolved>("proxy_resolve", proxyArgs(opts));
}

/**
 * Output callback (plan-15): `done` must be called once xterm has consumed
 * `bytes`; the next fetch waits for it. A pane that falls too far behind does
 * not slow the child (plan-19): its next fetch is a snapshot of the screen,
 * starting with a reset, instead of the bytes it missed.
 */
export type OnData = (bytes: Uint8Array, done: () => void) => void;

export interface PtySession {
  id: number;
  write(data: string): Promise<void>;
  resize(cols: number, rows: number): Promise<void>;
  /** This terminal is done with the shell, which keeps running (plan-19). */
  detach(): Promise<void>;
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

// Output is pulled (plan-15): Rust emits `pty-data` once when a pane's queue
// goes from empty to non-empty, and the session drains it with `pty_read`
// until empty. One app-wide listener, fanned out by PTY id to the drain.
const dataHandlers = new Map<number, () => void>();
let dataListener: Promise<UnlistenFn> | null = null;

function ensureDataListener() {
  dataListener ??= listen<PtyData>("pty-data", (ev) => dataHandlers.get(ev.payload.id)?.());
}

/** Mirrors `AGENTS` in src-tauri/core/src/agents.rs — display only; Rust decides. */
export const AGENTS = ["claude", "codex", "aider", "gemini", "copilot", "amp", "goose", "opencode"];

interface PaneAgent {
  id: number;
  agent: string | null;
}

// One app-wide listener for `pty-agent`, fanned out by PTY id. The PTY id is
// not the pane id: the pane that owns the session maps it (TerminalPane).
const agentHandlers = new Map<number, Set<(agent: string | null) => void>>();
// The last answer seen per id: one that arrives while `pty_open` is in flight
// is newer than what its reply says.
const latestAgents = new Map<number, string | null>();
let agentListener: Promise<UnlistenFn> | null = null;

function ensureAgentListener() {
  agentListener ??= listen<PaneAgent>("pty-agent", (ev) => {
    const { id, agent } = ev.payload;
    latestAgents.set(id, agent);
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
    latestAgents.delete(id);
    if (handlers) handlers.forEach((cb) => cb(code));
    else earlyExits.set(id, code);
  });
}

// The daemon's connection went away (plan-19 5.10): every shell this page knew
// is gone with it, so every id-keyed map is stale. Panes show a Restart.
const lostHandlers = new Set<() => void>();
let lostListener: Promise<UnlistenFn> | null = null;

function ensureLostListener() {
  lostListener ??= listen("pty-daemon-lost", () => {
    dataHandlers.clear();
    exitHandlers.clear();
    earlyExits.clear();
    agentHandlers.clear();
    latestAgents.clear();
    lostHandlers.forEach((cb) => cb());
  });
}

/** Fires when the background service stops. Returns an unsubscribe function. */
export function onDaemonLost(cb: () => void): () => void {
  ensureLostListener();
  lostHandlers.add(cb);
  return () => {
    lostHandlers.delete(cb);
  };
}

export type Rgb = [number, number, number];

/** `#rgb`, `#rrggbb` or `rgb(r, g, b)`, as a computed CSS token may read. */
export function parseColor(css: string): Rgb | null {
  const s = css.trim();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(s);
  if (short) return [1, 2, 3].map((i) => parseInt(short[i] + short[i], 16)) as Rgb;
  const long = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(s);
  if (long) return [1, 2, 3].map((i) => parseInt(long[i], 16)) as Rgb;
  const fn = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(s);
  if (fn) return [1, 2, 3].map((i) => Math.min(255, Number(fn[i]))) as Rgb;
  return null;
}

/**
 * The terminal colours, so a shell asking for them (OSC 10/11) while no window
 * is attached gets this theme's answer (plan-19 section 3).
 */
export function setTheme(fg: Rgb, bg: Rgb): Promise<void> {
  return invoke("pty_theme", { fg, bg });
}

/**
 * Just before the window goes: `keep` leaves every shell running in the
 * background service, otherwise closing ends them (plan-19 section 8).
 */
export function setCloseMode(keep: boolean): Promise<void> {
  return invoke("pty_close_mode", { keep });
}

/**
 * Before an update installs: end every shell and the background service, and
 * wait for it to go, since the installer replaces its exe (plan-19 7.4).
 */
export function shutdownDaemon(): Promise<boolean> {
  return invoke<boolean>("pty_shutdown");
}

/**
 * Sessions on a background service of another version, if Rust found one
 * (plan-19 7.5). Asked on mount: the first connect happens before any listener.
 */
export function daemonMismatch(): Promise<number | null> {
  return invoke<number | null>("pty_mismatch");
}

/** Fires when a connect finds a background service of another version. */
export function onDaemonMismatch(cb: (sessions: number) => void): Promise<UnlistenFn> {
  return listen<number>("pty-daemon-mismatch", (e) => cb(e.payload));
}

/** End the other version's service and its sessions; start this one's. */
export function replaceDaemon(): Promise<void> {
  return invoke("pty_replace_daemon");
}

/** One row of `pty_list`: what is running, whatever the page knows about. */
export function listSessions(): Promise<SessionInfo[]> {
  return invoke<SessionInfo[]>("pty_list");
}

/** Boot found an exited session it has nothing to show for. */
export function forgetSession(key: string): Promise<void> {
  return invoke("pty_forget", { key });
}

// Opens and kills overlap: `pty_open` can be blocked on the proxy for a while,
// and commands do not run in the order they were sent.
const opening = new Map<string, Promise<unknown>>();
const killing = new Set<Promise<unknown>>();

/**
 * End a pane's shell. Waits for an open of that key still in flight, or the kill
 * would find nothing and the open would then start a shell nobody wanted.
 */
export function killSession(key: string): Promise<void> {
  const done = Promise.resolve(opening.get(key))
    .catch(() => {})
    .then(() => invoke<void>("pty_kill", { key }))
    .finally(() => killing.delete(done));
  killing.add(done);
  return done;
}

interface OpenedPane {
  id: number;
  client: number;
  exited: boolean;
  code: number | null;
  agent: string | null;
}

async function open(opts: OpenOptions): Promise<OpenedPane> {
  // A pane just closed may be what makes room under the cap in Rust.
  await Promise.allSettled([...killing]);
  return invoke<OpenedPane>("pty_open", {
    key: opts.key,
    cwd: opts.cwd,
    cols: opts.cols,
    rows: opts.rows,
    shell: opts.shell ?? null,
    ...proxyArgs(opts),
  });
}

export async function openPty(opts: OpenOptions, onData: OnData): Promise<PtySession> {
  ensureLostListener();
  ensureExitListener();
  ensureAgentListener();
  ensureDataListener();
  const pending = open(opts);
  opening.set(opts.key, pending);
  const opened = await pending.finally(() => {
    if (opening.get(opts.key) === pending) opening.delete(opts.key);
  });
  const { id, client } = opened;
  // Exited before this page looked: a tombstone. Delivered like any early exit.
  if (opened.exited) earlyExits.set(id, opened.code);

  // The drain: one loop per session, fetching until the queue is empty. A
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
        const bytes = new Uint8Array(await invoke<ArrayBuffer>("pty_read", { id, client }));
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
  // The shell may have written (and signalled) before `pty_open` replied:
  // that signal is folded into Rust's `notified` flag, so one unconditional
  // drain covers it and nothing before this point can be lost. After an attach
  // this first read is the snapshot.
  void drain();

  return {
    id,
    write: (data) => invoke("pty_write", { id, data }),
    resize: (cols, rows) => invoke("pty_resize", { id, cols, rows }),
    detach: () => {
      // Stop fetching and stop listening. Another terminal may already have
      // this id, as with React's double effect.
      live = false;
      if (dataHandlers.get(id) === drain) dataHandlers.delete(id);
      return invoke("pty_detach", { id, client });
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
      // Attaching to a shell whose agent is already running: the poller only
      // reports changes, so this is the only time this page hears about it.
      const current = latestAgents.has(id) ? latestAgents.get(id)! : opened.agent;
      if (current !== null) cb(current);
      return () => {
        agentHandlers.get(id)?.delete(cb);
        if (agentHandlers.get(id)?.size === 0) agentHandlers.delete(id);
      };
    },
  };
}
