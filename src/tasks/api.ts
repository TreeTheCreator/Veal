// Thin wrapper over the task commands (src-tauri/src/tasks.rs + the Inbox in
// db.rs), mirroring fs.ts. `root === null` is the Inbox; a project root is
// confined to pinned projects on the Rust side. Every mutation returns the
// whole list, so the store just replaces what it has.
//
// Lives under src/tasks/ rather than as src/tasks.ts: `./tasks` would then
// resolve to the file, not the directory.
import { invoke } from "@tauri-apps/api/core";

export type Status = "todo" | "doing" | "done";

export const STATUSES: Status[] = ["todo", "doing", "done"];

export interface Task {
  id: string;
  title: string;
  status: Status;
  created: string;
  /** Anything else in the file (an agent's `notes`, say) — kept, never shown. */
  [extra: string]: unknown;
}

export const tasks = {
  list: (root: string | null) => invoke<Task[]>("tasks_list", { root }),
  add: (root: string | null, title: string, created: string) =>
    invoke<Task[]>("tasks_add", { root, title, created }),
  setStatus: (root: string | null, id: string, status: Status) =>
    invoke<Task[]>("tasks_set_status", { root, id, status }),
  setTitle: (root: string | null, id: string, title: string) =>
    invoke<Task[]>("tasks_set_title", { root, id, title }),
  /** ↑ (-1) / ↓ (+1) within the task's column; a no-op at the edge. */
  shift: (root: string | null, id: string, delta: -1 | 1) => invoke<Task[]>("tasks_shift", { root, id, delta }),
  remove: (root: string | null, id: string) => invoke<Task[]>("tasks_remove", { root, id }),
};
