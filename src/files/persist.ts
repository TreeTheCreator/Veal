// Plan-11: the Files tree's expanded directories and open file per project,
// saved to the `settings` table (`files.<projectId>`) so a relaunch shows
// the tree as it was. Written debounced like the pane layout; restored by
// `ensure` when "Restore layout on launch" is on.
import { db } from "../db";
import { useFiles, type ProjectFiles } from "./store";
import { flatten, pathKey } from "./tree";

export const FILES_KEY_PREFIX = "files.";
export const SAVE_DEBOUNCE_MS = 250;

export interface SavedFiles {
  /** Real paths of expanded directories. */
  expanded: string[];
  open: string | null;
}

export function filesKey(projectId: number): string {
  return `${FILES_KEY_PREFIX}${projectId}`;
}

export function serializeFiles(s: SavedFiles): string {
  return JSON.stringify({ expanded: s.expanded, open: s.open });
}

/** Null for anything malformed; an empty string is "nothing saved". */
export function parseFiles(json: string | undefined | null): SavedFiles | null {
  if (!json) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;
  const { expanded, open } = raw as Record<string, unknown>;
  if (!Array.isArray(expanded) || !expanded.every((p) => typeof p === "string" && p)) return null;
  if (open !== null && (typeof open !== "string" || !open)) return null;
  return { expanded: expanded as string[], open: open as string | null };
}

/** Every `files.<id>` entry in the raw settings map. */
export function parseAllFiles(settings: Record<string, string>): Map<number, SavedFiles> {
  const out = new Map<number, SavedFiles>();
  for (const [key, value] of Object.entries(settings)) {
    if (!key.startsWith(FILES_KEY_PREFIX)) continue;
    const id = Number(key.slice(FILES_KEY_PREFIX.length));
    const saved = parseFiles(value);
    if (Number.isInteger(id) && saved) out.set(id, saved);
  }
  return out;
}

/**
 * The real paths of the expanded directories, or null while the answer
 * would be lossy (plan-12): `flatten` cannot emit a directory whose parent
 * is still unlisted, so right after a restore — before the root listing
 * has arrived — it would say "nothing expanded" and that must not be
 * saved. Once no listing is loading, a key with no row is a directory
 * that is gone, and dropping it is right.
 */
export function settledExpanded(p: Pick<ProjectFiles, "root" | "dirs" | "expanded">): string[] | null {
  const rows = flatten(p.root, p.dirs, p.expanded).filter((r) => r.kind === "dir" && r.expanded);
  const seen = new Set(rows.map((r) => pathKey(r.path)));
  const missing = [...p.expanded].some((k) => !seen.has(k));
  const loading = [...p.dirs.values()].some((d) => d.status === "loading");
  return missing && loading ? null : rows.map((r) => r.path);
}

let started = false;

/**
 * Writes a project's tree state whenever its expanded set, open file or a
 * listing changes — the listing so a save skipped as lossy is retried once
 * the tree has settled. Call once, after the restore, so nothing is saved
 * before it. An unchanged string is not written again.
 */
export function startFilesPersistence(): void {
  if (started) return;
  started = true;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  const written = new Map<number, string>();
  useFiles.subscribe((s, prev) => {
    for (const [id, p] of s.byProject) {
      const before = prev.byProject.get(id);
      if (before && before.expanded === p.expanded && before.open === p.open && before.dirs === p.dirs) continue;
      clearTimeout(timers.get(id));
      timers.set(
        id,
        setTimeout(() => {
          const cur = useFiles.getState().byProject.get(id);
          if (!cur) return;
          const expanded = settledExpanded(cur);
          if (expanded === null) return; // the next listing re-arms this
          const value = serializeFiles({ expanded, open: cur.open });
          if (written.get(id) === value) return;
          written.set(id, value);
          db.settingsSet(filesKey(id), value).catch((e) => console.warn("files save", e));
        }, SAVE_DEBOUNCE_MS),
      );
    }
    for (const id of written.keys()) if (!s.byProject.has(id)) written.delete(id);
  });
}
