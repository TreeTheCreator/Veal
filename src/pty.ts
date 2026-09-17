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
}

interface PtyExit {
  id: number;
  code: number | null;
}

// One app-wide listener for `pty-exit`, fanned out by pane id.
const exitHandlers = new Map<number, Set<(code: number | null) => void>>();
let exitListener: Promise<UnlistenFn> | null = null;

function ensureExitListener() {
  exitListener ??= listen<PtyExit>("pty-exit", (ev) => {
    const { id, code } = ev.payload;
    const handlers = exitHandlers.get(id);
    exitHandlers.delete(id);
    handlers?.forEach((cb) => cb(code));
  });
}

export async function spawnPty(
  opts: SpawnOptions,
  onData: (bytes: Uint8Array) => void,
): Promise<PtySession> {
  ensureExitListener();
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
      let set = exitHandlers.get(id);
      if (!set) exitHandlers.set(id, (set = new Set()));
      set.add(cb);
      return () => {
        exitHandlers.get(id)?.delete(cb);
      };
    },
  };
}
