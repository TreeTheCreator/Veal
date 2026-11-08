import { afterEach, beforeEach, describe, expect, it } from "vitest";
import ptyRs from "../../src-tauri/core/src/pty.rs?raw";
import { reconcile, removedSessions, startSessionKiller, type SessionInfo } from "./sessions";
import { useLayout } from "./store";
import { insert, PANE_CAP, panes, type LayoutNode, type PaneNode } from "./tree";

const CWD = "C:\\veal";
const pane = (id: number): PaneNode => ({ type: "pane", id, cwd: CWD, session: `k${id}` });
const running = (key: string, cwd = CWD): SessionInfo => ({ key, cwd, exited: false, code: null });
const exited = (key: string, code: number | null): SessionInfo => ({ key, cwd: CWD, exited: true, code });

function tree(...ids: number[]): LayoutNode | null {
  let t: LayoutNode | null = null;
  for (const id of ids) t = insert(t, pane(id));
  return t;
}
const keys = (t: LayoutNode | null) => panes(t).map((p) => p.session);

describe("reconcile (plan-19, section 5)", () => {
  it("keeps a pane whose shell is running", () => {
    const t = tree(1, 2);
    const r = reconcile(t, 3, [running("k1"), running("k2")]);
    expect(r.tree).toBe(t);
    expect(r.forget).toEqual([]);
    expect(r.nextId).toBe(3);
  });

  it("closes a pane whose shell exited with code 0, and forgets it", () => {
    const r = reconcile(tree(1, 2), 3, [running("k1"), exited("k2", 0)]);
    expect(keys(r.tree)).toEqual(["k1"]);
    expect(r.forget).toEqual(["k2"]);
  });

  it("keeps a pane whose shell exited with any other code, to show it", () => {
    for (const code of [3, null]) {
      const r = reconcile(tree(1), 2, [exited("k1", code)]);
      expect(keys(r.tree)).toEqual(["k1"]);
      expect(r.forget).toEqual([]);
    }
  });

  it("keeps a pane with no session, which spawns fresh", () => {
    const t = tree(1);
    expect(reconcile(t, 2, []).tree).toBe(t);
  });

  it("adds a pane for a running shell with none, in that shell's cwd", () => {
    const r = reconcile(tree(1), 2, [running("k1"), running("orphan", "C:\\other")]);
    const added = panes(r.tree).find((p) => p.session === "orphan")!;
    expect(added.cwd).toBe("C:\\other");
    expect(added.id).toBe(2);
    expect(r.nextId).toBe(3);
  });

  it("shows running shells even with no saved layout (restoreLayout off)", () => {
    const r = reconcile(null, 1, [running("a"), running("b")]);
    expect(keys(r.tree)).toEqual(["a", "b"]);
    expect(new Set(panes(r.tree).map((p) => p.id)).size).toBe(2);
  });

  it("forgets an exited session with no pane", () => {
    const r = reconcile(tree(1), 2, [running("k1"), exited("gone", 1), exited("also", 0)]);
    expect(keys(r.tree)).toEqual(["k1"]);
    expect(r.forget).toEqual(["gone", "also"]);
  });

  it("closing every pane can leave an empty layout", () => {
    expect(reconcile(tree(1), 2, [exited("k1", 0)]).tree).toBeNull();
  });

  it("adopts no further than the cap", () => {
    const r = reconcile(tree(1, 2, 3), 4, [running("x"), running("y")]);
    expect(panes(r.tree)).toHaveLength(PANE_CAP);
    expect(keys(r.tree)).toContain("x");
  });
});

describe("the session cap", () => {
  it("PANE_CAP and Rust's MAX_SESSIONS agree", () => {
    expect(Number(ptyRs.match(/MAX_SESSIONS: usize = (\d+)/)![1])).toBe(PANE_CAP);
  });
});

describe("the kill subscriber (plan-19, section 5)", () => {
  const killed: string[] = [];
  let stop: () => void;

  beforeEach(() => {
    killed.length = 0;
    useLayout.setState({ tree: null, focused: null, nextId: 1, agents: new Map() });
    stop = startSessionKiller((key) => killed.push(key));
  });
  afterEach(() => stop());

  it("kills the shell of a closed pane, and no other", () => {
    const s = useLayout.getState();
    const a = s.addPane(CWD)!;
    s.addPane(CWD);
    const key = panes(useLayout.getState().tree).find((p) => p.id === a)!.session;
    s.closePane(a);
    expect(killed).toEqual([key]);
  });

  it("does nothing on a boot's restore, a divider drag, a focus or an agent", () => {
    useLayout.setState({ tree: tree(1, 2), focused: 1, nextId: 3 });
    const s = useLayout.getState();
    s.setWeights([], 0, 0.5);
    s.focus(2);
    s.setAgent(1, "claude");
    expect(killed).toEqual([]);
  });

  it("a pane moving within the tree is not a removal", () => {
    expect(removedSessions(tree(1, 2, 3), tree(3, 1, 2))).toEqual([]);
    expect(removedSessions(tree(1, 2), null).sort()).toEqual(["k1", "k2"]);
    expect(removedSessions(null, tree(1))).toEqual([]);
  });

  it("a new pane gets a key of its own", () => {
    const s = useLayout.getState();
    s.addPane(CWD);
    s.addPane(CWD);
    const [a, b] = keys(useLayout.getState().tree);
    expect(a).toMatch(/^[\w-]{36}$/);
    expect(a).not.toBe(b);
  });
});
