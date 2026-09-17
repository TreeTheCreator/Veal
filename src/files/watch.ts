// Routes a watcher batch to what the Files view is showing (plan-07,
// decision 5): a changed path re-lists its parent if that directory is
// listed and on screen, re-lists itself if it is an expanded directory, and
// flags the open file. Nothing not displayed is read. Pure; vitest-tested.
import { pathKey, type Row } from "./tree";
import { flatten } from "./tree";
import type { ProjectFiles } from "./store";

export interface Affected {
  /** Real (un-folded) directory paths to re-list, deduplicated. */
  dirs: string[];
  /** The open file is among the changed paths. */
  file: boolean;
}

/** `C:\a\b\c` → `C:\a\b`; either separator; `null` at a root. */
export function parentPath(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (i <= 0) return null;
  const parent = trimmed.slice(0, i);
  return /^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent;
}

export function affected(p: ProjectFiles, paths: string[]): Affected {
  const rootKey = pathKey(p.root);
  // Real paths for the folded keys: the root plus every expanded row.
  const real = new Map<string, string>([[rootKey, p.root]]);
  for (const r of flatten(p.root, p.dirs, p.expanded) as Row[]) {
    if (r.kind === "dir" && r.expanded) real.set(pathKey(r.path), r.path);
  }
  const openKey = p.open ? pathKey(p.open) : null;

  const dirs = new Map<string, string>();
  let file = false;
  for (const path of paths) {
    const key = pathKey(path);
    if (key === openKey) file = true;
    const shown = (k: string) => (k === rootKey || p.expanded.has(k)) && p.dirs.has(k) && real.has(k);
    if (shown(key)) dirs.set(key, real.get(key)!);
    const parent = parentPath(path);
    if (parent) {
      const pk = pathKey(parent);
      if (shown(pk)) dirs.set(pk, real.get(pk)!);
    }
  }
  return { dirs: [...dirs.values()], file };
}
