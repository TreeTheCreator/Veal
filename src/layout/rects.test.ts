import { describe, expect, it } from "vitest";
import { computeRects, dragWeight } from "./rects";
import type { LayoutNode, PaneNode } from "./tree";

const pane = (id: number): PaneNode => ({ type: "pane", id, kind: "shell", cwd: "C:\\veal" });
const row = (weights: number[], ...children: LayoutNode[]): LayoutNode => ({ type: "split", dir: "row", weights, children });
const col = (weights: number[], ...children: LayoutNode[]): LayoutNode => ({ type: "split", dir: "col", weights, children });

describe("computeRects", () => {
  it("insets a lone pane by the padding", () => {
    const g = computeRects(pane(1), { w: 1000, h: 600 });
    expect(g.panes.get(1)).toEqual({ x: 10, y: 10, w: 980, h: 580 });
    expect(g.dividers).toEqual([]);
  });

  it("splits a [1,1] row around a 10px divider", () => {
    const g = computeRects(row([1, 1], pane(1), pane(2)), { w: 1000, h: 600 });
    expect(g.panes.get(1)).toEqual({ x: 10, y: 10, w: 485, h: 580 });
    expect(g.panes.get(2)).toEqual({ x: 505, y: 10, w: 485, h: 580 });
    expect(g.dividers).toEqual([
      { path: [], index: 0, dir: "row", span: 970, rect: { x: 495, y: 10, w: 10, h: 580 } },
    ]);
  });

  it("honours unequal weights", () => {
    const g = computeRects(row([3, 1], pane(1), pane(2)), { w: 1000, h: 600 });
    expect(g.panes.get(1)?.w).toBeCloseTo(727.5);
    expect(g.panes.get(2)?.w).toBeCloseTo(242.5);
  });

  it("lays out a column inside the row child's rect", () => {
    const g = computeRects(row([1, 1], col([1, 1], pane(1), pane(3)), pane(2)), { w: 1000, h: 600 });
    expect(g.panes.get(1)).toEqual({ x: 10, y: 10, w: 485, h: 285 });
    expect(g.panes.get(3)).toEqual({ x: 10, y: 305, w: 485, h: 285 });
    expect(g.dividers.map((d) => [d.path, d.index, d.dir])).toEqual([
      [[0], 0, "col"],
      [[], 0, "row"],
    ]);
    expect(g.dividers[0].rect).toEqual({ x: 10, y: 295, w: 485, h: 10 });
  });

  it("returns nothing for an empty tree", () => {
    const g = computeRects(null, { w: 1000, h: 600 });
    expect(g.panes.size).toBe(0);
  });
});

describe("dragWeight", () => {
  it("converts pixels to weight in the split's units", () => {
    expect(dragWeight([1, 1], 0, 100, 1000)).toBeCloseTo(1.2);
  });

  it("clamps each neighbour to 15% of the pair", () => {
    expect(dragWeight([1, 1], 0, -900, 1000)).toBeCloseTo(0.3);
    expect(dragWeight([1, 1], 0, 900, 1000)).toBeCloseTo(1.7);
  });

  it("only moves the pair it sits between", () => {
    // Row [1,1,2]: dragging divider 1 scales by the total (4), pair is 3.
    expect(dragWeight([1, 1, 2], 1, 250, 1000)).toBeCloseTo(2);
  });

  it("keeps the weight for a non-finite delta (plan-16)", () => {
    expect(dragWeight([1, 1], 0, NaN, 100)).toBe(1);
    expect(dragWeight([1, 1], 0, Infinity, 100)).toBe(1);
  });
});
