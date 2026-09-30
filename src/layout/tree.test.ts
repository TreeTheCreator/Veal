import { describe, expect, it } from "vitest";
import { insert, panes, remove, setWeights, type LayoutNode, type PaneNode } from "./tree";

const pane = (id: number): PaneNode => ({ type: "pane", id, cwd: "C:\\veal" });

// Compact shape for assertions: pane id, or [dir, ...children].
function shape(node: LayoutNode | null): unknown {
  if (!node) return null;
  if (node.type === "pane") return node.id;
  return [node.dir, ...node.children.map(shape)];
}

function four(): LayoutNode {
  let t: LayoutNode | null = null;
  for (let i = 1; i <= 4; i++) t = insert(t, pane(i));
  return t!;
}

describe("insert", () => {
  it("fills the root row to two, then splits the emptier side into a column", () => {
    let t: LayoutNode | null = null;
    t = insert(t, pane(1));
    expect(shape(t)).toBe(1);
    t = insert(t, pane(2));
    expect(shape(t)).toEqual(["row", 1, 2]);
    t = insert(t, pane(3));
    expect(shape(t)).toEqual(["row", ["col", 1, 3], 2]);
    t = insert(t, pane(4));
    expect(shape(t)).toEqual(["row", ["col", 1, 3], ["col", 2, 4]]);
    expect(panes(t).map((p) => p.id)).toEqual([1, 3, 2, 4]);
  });

  it("gives every new child weight 1", () => {
    const t = four();
    expect(t.type === "split" && t.weights).toEqual([1, 1]);
    const left = (t as { children: LayoutNode[] }).children[0];
    expect(left.type === "split" && left.weights).toEqual([1, 1]);
  });

  it("pushes into an existing column rather than nesting", () => {
    let t = four();
    t = insert(t, pane(5));
    expect(shape(t)).toEqual(["row", ["col", 1, 3, 5], ["col", 2, 4]]);
  });

  it("wraps a column root in a row so the grid comes back (plan-13)", () => {
    let t: LayoutNode | null = four();
    t = remove(remove(t, 1), 3);
    expect(shape(t)).toEqual(["col", 2, 4]);
    t = insert(t, pane(5));
    expect(shape(t)).toEqual(["row", ["col", 2, 4], 5]);
    t = insert(t, pane(6));
    expect(shape(t)).toEqual(["row", ["col", 2, 4], ["col", 5, 6]]);
  });
});

describe("remove", () => {
  it("collapses a single-child split and keeps the row weights", () => {
    let t: LayoutNode | null = setWeights(four(), [], 0, 1.5);
    t = remove(t, 3);
    expect(shape(t)).toEqual(["row", 1, ["col", 2, 4]]);
    expect(t!.type === "split" && t!.weights).toEqual([1.5, 0.5]);
  });

  it("returns null when the last pane goes", () => {
    let t: LayoutNode | null = insert(null, pane(1));
    t = insert(t, pane(2));
    t = remove(t, 1);
    expect(shape(t)).toBe(2);
    expect(remove(t, 2)).toBeNull();
  });

  it("returns the same tree when the id is unknown", () => {
    const t = four();
    expect(remove(t, 99)).toBe(t);
  });
});

describe("setWeights", () => {
  it("sets a nested split's weights by path", () => {
    const t = setWeights(four(), [1], 0, 0.4) as { children: LayoutNode[]; weights: number[] };
    expect(t.weights).toEqual([1, 1]);
    const right = t.children[1];
    expect(right.type === "split" && right.weights).toEqual([0.4, 1.6]);
  });
});
