// Thin wrapper over the pty_* commands so components never call `invoke` directly.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SpawnOptions {
  cwd: string;
  cols: number;
  rows: number;
  shell?: string;
}

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
    if (handlers) handlers.forEach((cb) => cb(code));
    else earlyExits.set(id, code);
  });
}

export async function spawnPty(
  opts: SpawnOptions,
  onData: (bytes: Uint8Array) => void,
): Promise<PtySession> {
  ensureExitListener();
  ensureAgentListener();
  // The Rust side sends raw bytes; they arrive as an ArrayBuffer.
  const out = new Channel<ArrayBuffer>();
  out.onmessage = (buf) => onData(new Uint8Array(buf));
  const id = await invoke<number>("pty_spawn", { ...opts, shell: opts.shell ?? null, out });
  return {
    id,
    write: (data) => invoke("pty_write", { id, data }),
    resize: (cols, rows) => invoke("pty_resize", { id, cols, rows }),
    kill: () => invoke("pty_kill", { id }),
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
