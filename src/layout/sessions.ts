// What ties panes to the shells behind them (plan-19, section 5). A shell's
// lifetime is not a terminal's: unmounting a pane only detaches, and the one way a
// shell is killed is its pane leaving the layout.
import { useLayout } from "./store";
import { insert, PANE_CAP, panes, remove, type LayoutNode, type PaneNode } from "./tree";

/** One row of `pty_list`. */
export interface SessionInfo {
  key: string;
  cwd: string;
  exited: boolean;
  code: number | null;
}

export interface Reconciled {
  tree: LayoutNode | null;
  nextId: number;
  /** Exited sessions with nothing left to show. */
  forget: string[];
}

/**
 * Boot, given the saved layout and what is running:
 *
 * | Pane's session          | Result                                         |
 * |-------------------------|------------------------------------------------|
 * | Running                 | The pane attaches; its snapshot restores it.   |
 * | Exited, code 0          | Close the pane, forget the session.            |
 * | Exited, any other code  | The pane shows its last screen and the code.   |
 * | Unknown                 | The pane spawns fresh in the saved cwd.        |
 * | Running, with no pane   | Add a pane for it. A running shell is never hidden. |
 * | Exited, with no pane    | Forget it.                                     |
 *
 * The cap holds on the Rust side too, so more running sessions than panes is not
 * expected; one that does not fit stays running and listed for the next boot.
 */
export function reconcile(tree: LayoutNode | null, nextId: number, sessions: SessionInfo[]): Reconciled {
  const byKey = new Map(sessions.map((s) => [s.key, s]));
  const forget: string[] = [];
  const seen = new Set<string>();
  let next = tree;
  for (const pane of panes(tree)) {
    seen.add(pane.session);
    const s = byKey.get(pane.session);
    if (s?.exited && s.code === 0) {
      next = remove(next, pane.id);
      forget.push(s.key);
    }
  }
  let id = nextId;
  for (const s of sessions) {
    if (seen.has(s.key)) continue;
    if (s.exited) {
      forget.push(s.key);
    } else if (panes(next).length < PANE_CAP) {
      const pane: PaneNode = { type: "pane", id: id++, cwd: s.cwd, session: s.key };
      next = insert(next, pane);
    }
  }
  return { tree: next, nextId: id, forget };
}

/** Sessions whose pane is in `prev` and not in `next`. */
export function removedSessions(prev: LayoutNode | null, next: LayoutNode | null): string[] {
  if (prev === next) return [];
  const kept = new Set(panes(next).map((p) => p.session));
  return panes(prev)
    .map((p) => p.session)
    .filter((key) => !kept.has(key));
}

/**
 * The one place a shell is killed: any pane leaving the layout, whatever removed
 * it. A pane unmounting, a page reloading and boot's own restore do none of that,
 * so none of them can end an agent. Call once. Returns the unsubscribe.
 */
export function startSessionKiller(kill: (key: string) => void): () => void {
  return useLayout.subscribe((s, prev) => {
    for (const key of removedSessions(prev.tree, s.tree)) kill(key);
  });
}
