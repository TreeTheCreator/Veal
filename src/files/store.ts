// Per-project Files state: which directories are expanded and listed, and
// which file is open. In memory only (plan-04, decision 7): switching projects
// keeps each project's tree, unpinning forgets it, relaunch starts collapsed.
import { create } from "zustand";
import { fs, type FileText } from "../fs";
import { flatten, pathKey, type DirState } from "./tree";

export type FileState =
  | { status: "loading" }
  | { status: "loaded"; text: string; size: number }
  | { status: "error"; error: string };

export interface ProjectFiles {
  root: string;
  dirs: Map<string, DirState>;
  expanded: Set<string>;
  open: string | null;
  file: FileState | null;
}

interface FilesState {
  byProject: Map<number, ProjectFiles>;
  /** Lists the root the first time a project is seen; a no-op afterwards. */
  ensure(projectId: number, root: string): void;
  toggleDir(projectId: number, path: string): void;
  openFile(projectId: number, path: string): void;
  /** Re-lists the root and every expanded directory, and re-reads the open file. */
  refresh(projectId: number): void;
  forget(projectId: number): void;
}

const empty = (root: string): ProjectFiles => ({
  root,
  dirs: new Map(),
  expanded: new Set(),
  open: null,
  file: null,
});

export const useFiles = create<FilesState>((set, get) => {
  /** Replaces one project's slice; the maps are copied so React sees a change. */
  const patch = (projectId: number, fn: (p: ProjectFiles) => Partial<ProjectFiles>) => {
    const cur = get().byProject.get(projectId);
    if (!cur) return;
    const next = new Map(get().byProject);
    next.set(projectId, { ...cur, ...fn(cur) });
    set({ byProject: next });
  };

  const setDir = (projectId: number, dir: string, state: DirState) =>
    patch(projectId, (p) => {
      const dirs = new Map(p.dirs);
      dirs.set(pathKey(dir), state);
      return { dirs };
    });

  const list = (projectId: number, dir: string) => {
    const prev = get().byProject.get(projectId)?.dirs.get(pathKey(dir));
    setDir(projectId, dir, { status: "loading", entries: prev?.entries });
    fs.list(dir).then(
      (entries) => setDir(projectId, dir, { status: "loaded", entries }),
      (e) => setDir(projectId, dir, { status: "error", error: String(e), entries: prev?.entries }),
    );
  };

  const read = (projectId: number, path: string) => {
    patch(projectId, () => ({ open: path, file: { status: "loading" } }));
    const still = () => pathKey(get().byProject.get(projectId)?.open ?? "") === pathKey(path);
    fs.read(path).then(
      (f: FileText) => still() && patch(projectId, () => ({ file: { status: "loaded", ...f } })),
      (e) => still() && patch(projectId, () => ({ file: { status: "error", error: String(e) } })),
    );
  };

  return {
    byProject: new Map(),

    ensure(projectId, root) {
      if (get().byProject.has(projectId)) return;
      const next = new Map(get().byProject);
      next.set(projectId, empty(root));
      set({ byProject: next });
      list(projectId, root);
    },

    toggleDir(projectId, path) {
      const p = get().byProject.get(projectId);
      if (!p) return;
      const key = pathKey(path);
      const expanded = new Set(p.expanded);
      if (expanded.has(key)) {
        expanded.delete(key);
        patch(projectId, () => ({ expanded }));
      } else {
        // Expanding always re-lists, so a directory is never staler than its last open.
        expanded.add(key);
        patch(projectId, () => ({ expanded }));
        list(projectId, path);
      }
    },

    openFile(projectId, path) {
      if (get().byProject.has(projectId)) read(projectId, path);
    },

    refresh(projectId) {
      const p = get().byProject.get(projectId);
      if (!p) return;
      // The map keys are case-folded; the rows carry the real paths.
      const dirs = flatten(p.root, p.dirs, p.expanded).filter((r) => r.expanded);
      list(projectId, p.root);
      for (const r of dirs) list(projectId, r.path);
      if (p.open) read(projectId, p.open);
    },

    forget(projectId) {
      if (!get().byProject.has(projectId)) return;
      const next = new Map(get().byProject);
      next.delete(projectId);
      set({ byProject: next });
    },
  };
});

export const selectProject = (id: number | null) => (s: FilesState) =>
  id === null ? undefined : s.byProject.get(id);
