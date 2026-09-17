import { describe, expect, it } from "vitest";
import { parseLayout, serializeLayout } from "./persist";
import { insert, type LayoutNode, type PaneNode } from "./tree";

const CWD = "C:\\Users\\user\\veal";

const pane = (id: number, kind: "shell" | "agent" = "shell"): PaneNode => ({ type: "pane", id, kind, cwd: CWD });

function fourPanes(): LayoutNode {
  let t: LayoutNode | null = null;
  for (let i = 1; i <= 4; i++) t = insert(t, pane(i, i % 2 ? "shell" : "agent"));
  return t!;
}

describe("parseLayout", () => {
  it("round-trips the (1/3) | (2/4) tree", () => {
    const snap = { tree: fourPanes(), focused: 3, nextId: 5 };
    expect(parseLayout(serializeLayout(snap))).toEqual(snap);
  });

  it("round-trips an empty layout", () => {
    expect(parseLayout(serializeLayout({ tree: null, focused: null, nextId: 7 }))).toEqual({
      tree: null,
      focused: null,
      nextId: 1,
    });
  });

  it("rejects malformed input", () => {
    expect(parseLayout(undefined)).toBeNull();
    expect(parseLayout("")).toBeNull();
    expect(parseLayout("{not json")).toBeNull();
    expect(parseLayout("[]")).toBeNull();
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: 1, kind: "robot", cwd: CWD } }))).toBeNull();
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: 1, kind: "shell", cwd: "" } }))).toBeNull();
    expect(
      parseLayout(JSON.stringify({ tree: { type: "split", dir: "row", weights: [1], children: [pane(1), pane(2)] } })),
    ).toBeNull();
    expect(
      parseLayout(JSON.stringify({ tree: { type: "split", dir: "row", weights: [1, 0], children: [pane(1), pane(2)] } })),
    ).toBeNull();
  });

  it("rejects a tree over the cap or with duplicate ids", () => {
    const five: LayoutNode = {
      type: "split",
      dir: "row",
      weights: [1, 1, 1, 1, 1],
      children: [1, 2, 3, 4, 5].map((i) => pane(i)),
    };
    expect(parseLayout(JSON.stringify({ tree: five }))).toBeNull();
    const dup: LayoutNode = { type: "split", dir: "row", weights: [1, 1], children: [pane(1), pane(1)] };
    expect(parseLayout(JSON.stringify({ tree: dup }))).toBeNull();
  });

  it("recomputes nextId from the leaves and repairs focus", () => {
    const parsed = parseLayout(JSON.stringify({ tree: fourPanes(), focused: 99, nextId: 2 }));
    expect(parsed?.nextId).toBe(5);
    expect(parsed?.focused).toBe(1);
  });
});
