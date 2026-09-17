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
  /** Detected agent per pane id (plan-08); runtime only, never in the snapshot. */
  agents: Map<number, string>;
  /** Returns the new pane's id, or null when the soft cap is reached. */
  addPane(kind: PaneKind, cwd: string): number | null;
  closePane(id: number): void;
  flipKind(id: number): void;
  /** From `pty-agent`: `null` clears. */
  setAgent(id: number, agent: string | null): void;
  focus(id: number): void;
  setWeights(path: Path, index: number, a: number): void;
}

export const useLayout = create<LayoutState>((set, get) => ({
  tree: null,
  focused: null,
  nextId: 1,
  agents: new Map(),

  addPane(kind, cwd) {
    const { tree, nextId } = get();
    if (panes(tree).length >= PANE_CAP) return null;
    const pane: PaneNode = { type: "pane", id: nextId, kind, cwd };
    set({ tree: insert(tree, pane), focused: nextId, nextId: nextId + 1 });
    return nextId;
  },

  closePane(id) {
    const { tree, focused, agents } = get();
    const next = remove(tree, id);
    if (next === tree) return;
    const rest = new Map(agents);
    rest.delete(id);
    set({
      tree: next,
      focused: focused === id ? (panes(next)[0]?.id ?? null) : focused,
      agents: rest,
    });
  },

  flipKind(id) {
    const { tree } = get();
    const current = panes(tree).find((p) => p.id === id);
    if (!current) return;
    set({ tree: setKind(tree, id, current.kind === "agent" ? "shell" : "agent") });
  },

  setAgent(id, agent) {
    const { agents } = get();
    if ((agents.get(id) ?? null) === agent) return;
    const next = new Map(agents);
    if (agent === null) next.delete(id);
    else next.set(id, agent);
    set({ agents: next });
  },

  focus(id) {
    if (get().focused !== id) set({ focused: id });
  },

  setWeights(path, index, a) {
    set({ tree: setWeights(get().tree, path, index, a) });
  },
}));

export const selectPaneCount = (s: LayoutState) => panes(s.tree).length;
