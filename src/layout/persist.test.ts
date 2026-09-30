import { afterEach, describe, expect, it, vi } from "vitest";
import { flushLayout, parseLayout, SAVE_DEBOUNCE_MS, serializeLayout, startLayoutPersistence } from "./persist";
import { useLayout } from "./store";
import { insert, type LayoutNode, type PaneNode } from "./tree";

const settingsSet = vi.fn(async (_key: string, _value: string) => {});
vi.mock("../db", () => ({ db: { settingsSet: (k: string, v: string) => settingsSet(k, v) } }));

const CWD = "C:\\Users\\user\\veal";

const pane = (id: number): PaneNode => ({ type: "pane", id, cwd: CWD });

function fourPanes(): LayoutNode {
  let t: LayoutNode | null = null;
  for (let i = 1; i <= 4; i++) t = insert(t, pane(i));
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
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: 1, cwd: "" } }))).toBeNull();
    // Plan-16: ids are positive safe integers.
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: -5, cwd: CWD } }))).toBeNull();
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: 0, cwd: CWD } }))).toBeNull();
    expect(parseLayout(JSON.stringify({ tree: { type: "pane", id: 1e300, cwd: CWD } }))).toBeNull();
    expect(
      parseLayout(JSON.stringify({ tree: { type: "split", dir: "row", weights: [1], children: [pane(1), pane(2)] } })),
    ).toBeNull();
    expect(
      parseLayout(JSON.stringify({ tree: { type: "split", dir: "row", weights: [1, 0], children: [pane(1), pane(2)] } })),
    ).toBeNull();
  });

  it("drops a legacy kind (plan-17)", () => {
    const legacy = {
      tree: {
        type: "split",
        dir: "row",
        weights: [1, 1],
        children: [
          { type: "pane", id: 1, kind: "agent", cwd: CWD },
          { type: "pane", id: 2, kind: "robot", cwd: CWD },
        ],
      },
      focused: 1,
      nextId: 3,
    };
    const snap = parseLayout(JSON.stringify(legacy));
    expect(snap?.tree).toEqual({ type: "split", dir: "row", weights: [1, 1], children: [pane(1), pane(2)] });
    expect(serializeLayout(snap!)).not.toContain("kind");
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

describe("flushLayout (plan-16)", () => {
  afterEach(() => vi.useRealTimers());

  it("writes a pending change once, at once, and the timer does not write again", async () => {
    vi.useFakeTimers();
    startLayoutPersistence();
    useLayout.getState().addPane(CWD);
    expect(settingsSet).not.toHaveBeenCalled();
    await flushLayout();
    expect(settingsSet).toHaveBeenCalledTimes(1);
    expect(settingsSet.mock.calls[0][0]).toBe("layout");
    expect(parseLayout(settingsSet.mock.calls[0][1])?.tree?.type).toBe("pane");
    vi.advanceTimersByTime(SAVE_DEBOUNCE_MS * 2);
    expect(settingsSet).toHaveBeenCalledTimes(1);
  });

  it("is a no-op with nothing pending", async () => {
    settingsSet.mockClear();
    await flushLayout();
    expect(settingsSet).not.toHaveBeenCalled();
  });
});
