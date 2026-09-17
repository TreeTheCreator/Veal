// After a rename or delete, the per-project maps keyed by path (plan-04's
// `dirs` / `expanded`, plan-06's `editing` / `drafts`, the open file) must
// follow the entry and everything under it. Pure; vitest-tested.
import type { Draft, ProjectFiles } from "./store";
import { pathKey, type DirState } from "./tree";

/** True when `key` is `underKey` or lies below it. */
export function isUnder(key: string, underKey: string): boolean {
  return key === underKey || key.startsWith(underKey.endsWith("\\") ? underKey : `${underKey}\\`);
}

/** `C:\a\src\x.ts` with `C:\a\src` → `C:\a\lib` gives `C:\a\lib\x.ts` (the real path, not the key). */
function moved(path: string, from: string, to: string): string {
  const key = pathKey(path);
  const fromKey = pathKey(from);
  if (key === fromKey) return to;
  return to + path.slice(from.replace(/[\\/]+$/, "").length);
}

/** Every path-keyed entry at or under `from` now lives at `to`. */
export function rekeyProject(p: ProjectFiles, from: string, to: string): ProjectFiles {
  const fromKey = pathKey(from);
  const move = (key: string) => (isUnder(key, fromKey) ? pathKey(to + key.slice(fromKey.length)) : key);

  const dirs = new Map<string, DirState>();
  for (const [k, v] of p.dirs) dirs.set(move(k), v);
  const expanded = new Set<string>();
  for (const k of p.expanded) expanded.add(move(k));
  const editing = new Set<string>();
  for (const k of p.editing) editing.add(move(k));
  const drafts = new Map<string, Draft>();
  for (const [k, d] of p.drafts) {
    drafts.set(move(k), isUnder(k, fromKey) ? { ...d, path: moved(d.path, from, to) } : d);
  }
  const open = p.open && isUnder(pathKey(p.open), fromKey) ? moved(p.open, from, to) : p.open;
  return { ...p, dirs, expanded, editing, drafts, open };
}

/** Every path-keyed entry at or under `path` is gone; the open file closes if it was inside. */
export function dropUnder(p: ProjectFiles, path: string): ProjectFiles {
  const underKey = pathKey(path);
  const keep = (k: string) => !isUnder(k, underKey);
  const dirs = new Map([...p.dirs].filter(([k]) => keep(k)));
  const expanded = new Set([...p.expanded].filter(keep));
  const editing = new Set([...p.editing].filter(keep));
  const drafts = new Map([...p.drafts].filter(([k]) => keep(k)));
  const inside = p.open !== null && isUnder(pathKey(p.open), underKey);
  return { ...p, dirs, expanded, editing, drafts, open: inside ? null : p.open, file: inside ? null : p.file };
}
