// Per-project Files state: which directories are expanded and listed, which
// file is open, which files are in edit mode and which carry unsaved drafts.
// In memory only (plan-04, decision 7): switching projects keeps each
// project's tree and drafts, unpinning forgets it, relaunch starts collapsed.
import type { EditorState } from "@codemirror/state";
import { create } from "zustand";
import { fs, type FileText } from "../fs";
import { useUi } from "../ui";
import { flatten, pathKey, type DirState } from "./tree";
import { affected } from "./watch";

/** A watcher batch (`fs-changed`); `paths: null` means refresh everything shown. */
export interface Batch {
  projectId: number;
  paths: string[] | null;
}

export type FileState =
  | { status: "loading" }
  | {
      status: "loaded";
      /** Text as loaded — never the live document. Changes only when a read replaces the file. */
      text: string;
      size: number;
      mtime: number;
      bom: boolean;
      crlf: boolean;
      saving: boolean;
      conflict: "changed" | "deleted" | null;
      /** Bumped every time a read replaces the text; a save keeps it, so the editor knows when to rebuild. */
      gen: number;
    }
  | { status: "error"; error: string };

/** Exists only while the file is dirty; `state` is filled when the editor moves away. */
export interface Draft {
  state: EditorState | null;
  mtime: number;
  /** The real (un-folded) path, for the close guard's list. */
  path: string;
}

export interface ProjectFiles {
  root: string;
  dirs: Map<string, DirState>;
  expanded: Set<string>;
  open: string | null;
  file: FileState | null;
  /** Files whose Edit toggle is on (pathKey). */
  editing: Set<string>;
  /** Unsaved edits (pathKey → draft). A file is dirty iff it is here. */
  drafts: Map<string, Draft>;
}

export interface DirtyFile {
  projectId: number;
  root: string;
  path: string;
}

interface FilesState {
  byProject: Map<number, ProjectFiles>;
  /** Lists the root the first time a project is seen; a no-op afterwards. */
  ensure(projectId: number, root: string): void;
  toggleDir(projectId: number, path: string): void;
  openFile(projectId: number, path: string): void;
  /** Re-lists the root and every expanded directory, and quietly re-reads the open file. */
  refresh(projectId: number): void;
  /** Watcher batch: re-lists the shown directories a path touches, quietly re-reads the open file. */
  onChanged(batch: Batch): void;
  forget(projectId: number): void;
  /** Turns edit mode on/off for a file; off is ignored while it is dirty. */
  setEditing(projectId: number, path: string, on: boolean): void;
  /** First keystroke: records a draft against the loaded mtime. */
  markDirty(projectId: number, path: string): void;
  /** Keeps the live EditorState when the editor moves to another file. */
  stash(projectId: number, path: string, state: EditorState): void;
  /** Writes `text` back; resolves true on success. Sets `conflict` on a stale mtime. */
  save(projectId: number, path: string, text: string, force?: boolean): Promise<boolean>;
  /** Drops the draft and edit mode, then re-reads. */
  discard(projectId: number, path: string): void;
  dirtyFiles(): DirtyFile[];
}

const empty = (root: string): ProjectFiles => ({
  root,
  dirs: new Map(),
  expanded: new Set(),
  open: null,
  file: null,
  editing: new Set(),
  drafts: new Map(),
});

export const detectCrlf = (text: string): boolean => text.includes("\r\n");

let generation = 0;

const loaded = (f: FileText): FileState => ({
  status: "loaded",
  text: f.text,
  size: f.size,
  mtime: f.mtime,
  bom: f.bom,
  crlf: detectCrlf(f.text),
  saving: false,
  conflict: null,
  gen: ++generation,
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

  /** Patches the loaded file state if `path` is still the open file. */
  const patchFile = (projectId: number, path: string, fn: (f: FileState & { status: "loaded" }) => Partial<FileState>) =>
    patch(projectId, (p) => {
      if (pathKey(p.open ?? "") !== pathKey(path) || p.file?.status !== "loaded") return {};
      return { file: { ...p.file, ...fn(p.file) } as FileState };
    });

  /**
   * Opens `path` (a fresh `loading` state) or, `quiet`, re-reads the open
   * file in place: no `loading` flip — the editor unmounts on it — and an
   * unchanged mtime + size is a no-op, which is what swallows the watcher
   * event our own save raises so the cursor stays put.
   */
  const read = (projectId: number, path: string, quiet = false) => {
    const key = pathKey(path);
    const p = get().byProject.get(projectId);
    const draft = p?.drafts.get(key);
    const isOpen = pathKey(p?.open ?? "") === key && p?.file?.status === "loaded";
    // A dirty file keeps its loaded text through a re-read: only a
    // conflict is recorded, so the editor is never reset under the draft.
    const keepText = draft !== undefined && isOpen;
    const loadedNow = isOpen && quiet ? (p!.file as FileState & { status: "loaded" }) : null;
    if (!keepText && !loadedNow) patch(projectId, () => ({ open: path, file: { status: "loading" } }));
    const still = () => pathKey(get().byProject.get(projectId)?.open ?? "") === key;
    fs.read(path).then(
      (f: FileText) => {
        if (!still()) return;
        const conflict = draft && f.mtime !== draft.mtime ? "changed" : null;
        if (keepText) {
          if (conflict) patchFile(projectId, path, () => ({ conflict }));
        } else if (loadedNow && f.mtime === loadedNow.mtime && f.size === loadedNow.size) {
          // Unchanged (or our own write): keep the editor as it is.
        } else {
          // Coming back to a dirty file: the editor restores the stashed draft over this.
          patch(projectId, () => ({ file: { ...loaded(f), conflict } as FileState }));
        }
      },
      (e) => {
        if (!still()) return;
        if (keepText) {
          patchFile(projectId, path, () => ({ conflict: "deleted" }));
        } else if (draft) {
          // Deleted while we were away; keep the draft editable so it can be overwritten back.
          const text = draft.state?.doc.toString() ?? "";
          patch(projectId, () => ({
            file: { ...loaded({ text, size: 0, mtime: draft.mtime, bom: false }), conflict: "deleted" } as FileState,
          }));
        } else {
          patch(projectId, () => ({ file: { status: "error", error: String(e) } }));
        }
      },
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
      if (p.open) read(projectId, p.open, true);
    },

    onChanged({ projectId, paths }) {
      const p = get().byProject.get(projectId);
      if (!p) return;
      if (paths === null) {
        get().refresh(projectId);
        return;
      }
      const { dirs, file } = affected(p, paths);
      for (const d of dirs) list(projectId, d);
      if (file && p.open) read(projectId, p.open, true);
    },

    forget(projectId) {
      if (!get().byProject.has(projectId)) return;
      const next = new Map(get().byProject);
      next.delete(projectId);
      set({ byProject: next });
    },

    setEditing(projectId, path, on) {
      const key = pathKey(path);
      patch(projectId, (p) => {
        if (p.editing.has(key) === on || (!on && p.drafts.has(key))) return {};
        const editing = new Set(p.editing);
        if (on) editing.add(key);
        else editing.delete(key);
        return { editing };
      });
    },

    markDirty(projectId, path) {
      const key = pathKey(path);
      patch(projectId, (p) => {
        if (p.drafts.has(key) || !p.editing.has(key)) return {};
        const mtime = pathKey(p.open ?? "") === key && p.file?.status === "loaded" ? p.file.mtime : 0;
        const drafts = new Map(p.drafts);
        drafts.set(key, { state: null, mtime, path });
        return { drafts };
      });
    },

    stash(projectId, path, state) {
      const key = pathKey(path);
      patch(projectId, (p) => {
        const draft = p.drafts.get(key);
        if (!draft) return {};
        const drafts = new Map(p.drafts);
        drafts.set(key, { ...draft, state });
        return { drafts };
      });
    },

    async save(projectId, path, text, force = false) {
      const key = pathKey(path);
      const p = get().byProject.get(projectId);
      if (!p || pathKey(p.open ?? "") !== key || p.file?.status !== "loaded") return false;
      const { crlf, bom, mtime } = p.file;
      patchFile(projectId, path, () => ({ saving: true }));
      try {
        const w = await fs.write(path, text, { crlf, bom, expectedMtime: force ? null : mtime });
        patch(projectId, (q) => {
          const drafts = new Map(q.drafts);
          drafts.delete(key);
          const editing = new Set(q.editing);
          editing.delete(key);
          return { drafts, editing };
        });
        patchFile(projectId, path, () => ({ saving: false, conflict: null, size: w.size, mtime: w.mtime }));
        return true;
      } catch (e) {
        const msg = String(e);
        const conflict = msg === "changed on disk" ? "changed" : msg === "deleted on disk" ? "deleted" : null;
        patchFile(projectId, path, (f) => ({ saving: false, conflict: conflict ?? f.conflict }));
        if (!conflict) useUi.getState().showToast(`Save failed: ${msg}`);
        return false;
      }
    },

    discard(projectId, path) {
      const key = pathKey(path);
      patch(projectId, (p) => {
        const drafts = new Map(p.drafts);
        drafts.delete(key);
        const editing = new Set(p.editing);
        editing.delete(key);
        return { drafts, editing };
      });
      read(projectId, path);
    },

    dirtyFiles() {
      const out: DirtyFile[] = [];
      for (const [projectId, p] of get().byProject) {
        for (const d of p.drafts.values()) out.push({ projectId, root: p.root, path: d.path });
      }
      return out;
    },
  };
});

export const selectProject = (id: number | null) => (s: FilesState) =>
  id === null ? undefined : s.byProject.get(id);
