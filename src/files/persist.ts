// Plan-11: the Files tree's expanded directories and open file per project,
// saved to the `settings` table (`files.<projectId>`) so a relaunch shows
// the tree as it was. Written debounced like the pane layout; restored by
// `ensure` when "Restore layout on launch" is on.
import { db } from "../db";
import { useFiles } from "./store";
import { flatten } from "./tree";

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

let started = false;

/**
 * Writes a project's tree state whenever its expanded set or open file
 * changes. Call once, after the restore, so nothing is saved before it.
 */
export function startFilesPersistence(): void {
  if (started) return;
  started = true;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  useFiles.subscribe((s, prev) => {
    for (const [id, p] of s.byProject) {
      const before = prev.byProject.get(id);
      if (before && before.expanded === p.expanded && before.open === p.open) continue;
      clearTimeout(timers.get(id));
      timers.set(
        id,
        setTimeout(() => {
          const cur = useFiles.getState().byProject.get(id);
          if (!cur) return;
          const expanded = flatten(cur.root, cur.dirs, cur.expanded)
            .filter((r) => r.kind === "dir" && r.expanded)
            .map((r) => r.path);
          db.settingsSet(filesKey(id), serializeFiles({ expanded, open: cur.open })).catch((e) =>
            console.warn("files save", e),
          );
        }, SAVE_DEBOUNCE_MS),
      );
    }
  });
}
