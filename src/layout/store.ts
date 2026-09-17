// Layout state: the split tree plus which pane has focus. Persistence to
// SQLite (reopen where you left off) is step 4.
import { create } from "zustand";
import {
  insert,
  PANE_CAP,
  panes,
  remove,
  setKind,
  setWeights,
  type LayoutNode,
  type PaneKind,
  type PaneNode,
  type Path,
} from "./tree";

interface LayoutState {
  tree: LayoutNode | null;
  focused: number | null;
  nextId: number;
  /** Returns the new pane's id, or null when the soft cap is reached. */
  addPane(kind: PaneKind, cwd: string): number | null;
  closePane(id: number): void;
  flipKind(id: number): void;
  focus(id: number): void;
  setWeights(path: Path, index: number, a: number): void;
}

export const useLayout = create<LayoutState>((set, get) => ({
  tree: null,
  focused: null,
  nextId: 1,

  addPane(kind, cwd) {
    const { tree, nextId } = get();
    if (panes(tree).length >= PANE_CAP) return null;
    const pane: PaneNode = { type: "pane", id: nextId, kind, cwd };
    set({ tree: insert(tree, pane), focused: nextId, nextId: nextId + 1 });
    return nextId;
  },

  closePane(id) {
    const { tree, focused } = get();
    const next = remove(tree, id);
    if (next === tree) return;
    set({
      tree: next,
      focused: focused === id ? (panes(next)[0]?.id ?? null) : focused,
    });
  },

  flipKind(id) {
    const { tree } = get();
    const current = panes(tree).find((p) => p.id === id);
    if (!current) return;
    set({ tree: setKind(tree, id, current.kind === "agent" ? "shell" : "agent") });
  },

  focus(id) {
    if (get().focused !== id) set({ focused: id });
  },

  setWeights(path, index, a) {
    set({ tree: setWeights(get().tree, path, index, a) });
  },
}));

export const selectPaneCount = (s: LayoutState) => panes(s.tree).length;
