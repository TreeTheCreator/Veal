// Task lists per project plus the Inbox, in memory (plan-05). Nothing is
// optimistic: every mutation is a Rust read-modify-write that returns the
// whole list, and the view re-reads its scope each time it is shown, so the
// board always shows what is on disk. The watcher (plan-07) reloads a
// project's list when its `.veal/tasks.json` changes.
import { create } from "zustand";
import { pathKey } from "../files/tree";
import { tasks, type Status, type Task } from "./api";
import type { Scope } from "./board";

export type ListState =
  | { status: "loading" | "loaded"; tasks: Task[] }
  | { status: "error"; tasks: Task[]; error: string };

/** `projectId === null` is the Inbox (`root` is then null too). */
interface TasksState {
  scope: Scope;
  byProject: Map<number, ListState>;
  inbox: ListState;
  setScope(scope: Scope): void;
  load(projectId: number | null, root: string | null): Promise<void>;
  /** Resolve true on success; on failure the error is recorded and the list reloaded. */
  add(projectId: number | null, root: string | null, title: string, created: string): Promise<boolean>;
  setStatus(projectId: number | null, root: string | null, id: string, status: Status): Promise<boolean>;
  remove(projectId: number | null, root: string | null, id: string): Promise<boolean>;
  /**
   * Watcher batch for a project whose list has been loaded: re-reads it,
   * without the `loading` flip, if `.veal/tasks.json` (or `.veal` itself)
   * is among `paths`, or on `null`. A never-shown board is not read.
   */
  onChanged(projectId: number, root: string, paths: string[] | null): void;
  forget(projectId: number): void;
}

const TASKS_REL = ["\\.veal", "\\.veal\\tasks.json"];

/** True when `paths` touch the task file under `root`, or on `null` (refresh all). */
export function touchesTasks(root: string, paths: string[] | null): boolean {
  if (paths === null) return true;
  const targets = TASKS_REL.map((rel) => pathKey(root + rel));
  return paths.some((p) => targets.includes(pathKey(p)));
}

const EMPTY: ListState = { status: "loaded", tasks: [] };

export const useTasks = create<TasksState>((set, get) => {
  const current = (projectId: number | null): ListState =>
    projectId === null ? get().inbox : (get().byProject.get(projectId) ?? EMPTY);

  const put = (projectId: number | null, state: ListState) => {
    if (projectId === null) {
      set({ inbox: state });
    } else {
      const next = new Map(get().byProject);
      next.set(projectId, state);
      set({ byProject: next });
    }
  };

  /** Runs a command and replaces the list with its result; keeps stale tasks on failure. */
  const apply = async (
    projectId: number | null,
    root: string | null,
    op: () => Promise<Task[]>,
    reloadOnError: boolean,
  ): Promise<boolean> => {
    try {
      put(projectId, { status: "loaded", tasks: await op() });
      return true;
    } catch (e) {
      put(projectId, { status: "error", tasks: current(projectId).tasks, error: String(e) });
      // Re-read without a loading flip: a fresh list clears the error (the
      // task was simply gone), a failed one replaces it with the file's own.
      if (reloadOnError) void apply(projectId, root, () => tasks.list(root), false);
      return false;
    }
  };

  return {
    scope: "project",
    byProject: new Map(),
    inbox: EMPTY,

    setScope(scope) {
      set({ scope });
    },

    async load(projectId, root) {
      put(projectId, { status: "loading", tasks: current(projectId).tasks });
      await apply(projectId, root, () => tasks.list(root), false);
    },

    add(projectId, root, title, created) {
      return apply(projectId, root, () => tasks.add(root, title, created), true);
    },

    setStatus(projectId, root, id, status) {
      return apply(projectId, root, () => tasks.setStatus(root, id, status), true);
    },

    remove(projectId, root, id) {
      return apply(projectId, root, () => tasks.remove(root, id), true);
    },

    onChanged(projectId, root, paths) {
      if (!get().byProject.has(projectId) || !touchesTasks(root, paths)) return;
      void apply(projectId, root, () => tasks.list(root), false);
    },

    forget(projectId) {
      if (!get().byProject.has(projectId)) return;
      const next = new Map(get().byProject);
      next.delete(projectId);
      set({ byProject: next });
    },
  };
});

export const selectList = (projectId: number | null) => (s: TasksState) =>
  projectId === null ? s.inbox : (s.byProject.get(projectId) ?? EMPTY);
