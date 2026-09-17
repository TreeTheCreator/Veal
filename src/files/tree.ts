// The Files tree as a flat list of rows. The component renders rows, not a
// recursive node tree, so keyboard navigation is index arithmetic and there
// is no per-node component state. Pure; vitest-tested.
import type { Entry, EntryKind } from "../fs";
import { joinPath, normalizePath } from "../paths";

export type DirState =
  | { status: "loading"; entries?: Entry[] }
  | { status: "loaded"; entries: Entry[] }
  | { status: "error"; error: string; entries?: Entry[] };

export interface Row {
  /** Absolute path; the map key. */
  path: string;
  name: string;
  depth: number;
  kind: EntryKind;
  size: number;
  expanded: boolean;
  /** Directories only: the listing state. Files are always "loaded". */
  state: DirState["status"];
  error?: string;
}

/** Map key for a path: normalised and case-folded, like `samePath`. */
export function pathKey(path: string): string {
  return normalizePath(path).replace(/\//g, "\\").toLowerCase();
}

/**
 * Walks from `root`, descending only into expanded directories, and emits the
 * rows in display order. A directory that is still loading (or failed) is
 * emitted with that state and no children — unless an earlier listing is
 * still cached, in which case the stale children stay visible.
 */
export function flatten(root: string, dirs: Map<string, DirState>, expanded: Set<string>): Row[] {
  const rows: Row[] = [];
  const walk = (dir: string, depth: number) => {
    const state = dirs.get(pathKey(dir));
    for (const e of state?.entries ?? []) {
      const path = joinPath(dir, e.name);
      const key = pathKey(path);
      const isDir = e.kind === "dir";
      const isOpen = isDir && expanded.has(key);
      const sub = isDir ? dirs.get(key) : undefined;
      rows.push({
        path,
        name: e.name,
        depth,
        kind: e.kind,
        size: e.size,
        expanded: isOpen,
        state: isDir ? (sub?.status ?? "loading") : "loaded",
        error: sub?.status === "error" ? sub.error : undefined,
      });
      if (isOpen) walk(path, depth + 1);
    }
  };
  walk(root, 0);
  return rows;
}

/** Index of the row that contains `index`'s row (its parent directory), or -1. */
export function parentIndex(rows: Row[], index: number): number {
  const depth = rows[index]?.depth ?? 0;
  for (let i = index - 1; i >= 0; i--) if (rows[i].depth < depth) return i;
  return -1;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
