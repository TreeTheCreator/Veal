// Saves the split tree to the `layout` settings key so the app reopens where
// it left off, and restores it on boot. Restored panes respawn fresh shells in
// their saved cwd; scrollback is not part of the snapshot.
import { db } from "../db";
import { useLayout } from "./store";
import { PANE_CAP, panes, type LayoutNode, type PaneNode, type SplitNode } from "./tree";

export const LAYOUT_KEY = "layout";

/** Divider drags write on every pointermove; coalesce them. */
export const SAVE_DEBOUNCE_MS = 250;

export interface LayoutSnapshot {
  tree: LayoutNode | null;
  focused: number | null;
  nextId: number;
}

export function serializeLayout(s: LayoutSnapshot): string {
  return JSON.stringify({ tree: s.tree, focused: s.focused, nextId: s.nextId });
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

function parseNode(v: unknown): LayoutNode | null {
  if (!isRecord(v)) return null;
  if (v.type === "pane") {
    // A legacy `kind` (the manual Shell / Agent label, dropped in plan-17) is
    // ignored, so the next save writes panes without it.
    const { id, cwd } = v;
    // Positive safe integers only (plan-16): `-5` and `1e300` are integers
    // to `Number.isInteger`, and a corrupted snapshot is dropped, not repaired.
    if (!Number.isSafeInteger(id) || (id as number) <= 0) return null;
    if (typeof cwd !== "string" || !cwd) return null;
    const pane: PaneNode = { type: "pane", id: id as number, cwd };
    return pane;
  }
  if (v.type === "split") {
    const { dir, weights, children } = v;
    if ((dir !== "row" && dir !== "col") || !Array.isArray(weights) || !Array.isArray(children)) return null;
    if (children.length < 2 || weights.length !== children.length) return null;
    if (!weights.every((w) => typeof w === "number" && Number.isFinite(w) && w > 0)) return null;
    const kids = children.map(parseNode);
    if (kids.some((k) => k === null)) return null;
    const split: SplitNode = { type: "split", dir, weights: weights as number[], children: kids as LayoutNode[] };
    return split;
  }
  return null;
}

/**
 * Validates a saved snapshot. Returns null for anything malformed or over the
 * cap. `nextId` is always recomputed from the leaves so restored ids never
 * collide with panes opened later, and `focused` must name a live leaf.
 */
export function parseLayout(json: string | undefined | null): LayoutSnapshot | null {
  if (!json) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return null;
  }
  if (!isRecord(raw)) return null;
  const tree = raw.tree === null ? null : parseNode(raw.tree);
  if (raw.tree !== null && tree === null) return null;
  const leaves = panes(tree);
  if (leaves.length > PANE_CAP) return null;
  const ids = leaves.map((p) => p.id);
  if (new Set(ids).size !== ids.length) return null;
  const nextId = ids.length ? Math.max(...ids) + 1 : 1;
  const focused =
    typeof raw.focused === "number" && ids.includes(raw.focused) ? raw.focused : (ids[0] ?? null);
  return { tree, focused, nextId };
}

let started = false;
let timer: ReturnType<typeof setTimeout> | undefined;
/** The latest change the timer has not written yet. */
let unsaved: LayoutSnapshot | null = null;

function save(): Promise<void> {
  clearTimeout(timer);
  timer = undefined;
  const s = unsaved;
  unsaved = null;
  if (!s) return Promise.resolve();
  return db.settingsSet(LAYOUT_KEY, serializeLayout(s)).catch((e) => console.warn("layout save", e));
}

/**
 * Writes the pending change now, if there is one (plan-16): the close guard
 * calls this before the window goes, so a pane closed inside the debounce
 * is not back on the next launch.
 */
export function flushLayout(): Promise<void> {
  return save();
}

/**
 * Subscribes to the layout store and writes every change through, debounced.
 * Call once, after the restore, so the pre-restore empty state is never saved.
 */
export function startLayoutPersistence(): void {
  if (started) return;
  started = true;
  useLayout.subscribe((s, prev) => {
    if (s.tree === prev.tree && s.focused === prev.focused && s.nextId === prev.nextId) return;
    unsaved = s;
    clearTimeout(timer);
    timer = setTimeout(save, SAVE_DEBOUNCE_MS);
  });
}
