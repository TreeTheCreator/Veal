import { beforeEach, describe, expect, it } from "vitest";
import { serializeLayout } from "./persist";
import { useLayout } from "./store";

const A = "C:\\Users\\user\\veal";

beforeEach(() => {
  useLayout.setState({ tree: null, focused: null, nextId: 1, agents: new Map() });
});

describe("layout store agents (plan-08)", () => {
  it("setAgent adds, replaces and clears without touching the tree", () => {
    const s = useLayout.getState();
    const id = s.addPane("shell", A)!;
    const tree = useLayout.getState().tree;
    s.setAgent(id, "claude");
    expect(useLayout.getState().agents.get(id)).toBe("claude");
    expect(useLayout.getState().tree).toBe(tree);
    s.setAgent(id, "codex");
    expect(useLayout.getState().agents.get(id)).toBe("codex");
    s.setAgent(id, null);
    expect(useLayout.getState().agents.has(id)).toBe(false);
  });

  it("an unchanged answer is a no-op (same map identity)", () => {
    const s = useLayout.getState();
    const id = s.addPane("shell", A)!;
    s.setAgent(id, "claude");
    const map = useLayout.getState().agents;
    s.setAgent(id, "claude");
    expect(useLayout.getState().agents).toBe(map);
    s.setAgent(99, null);
    expect(useLayout.getState().agents).toBe(map);
  });

  it("closePane drops the pane's entry", () => {
    const s = useLayout.getState();
    const a = s.addPane("shell", A)!;
    const b = s.addPane("shell", A)!;
    s.setAgent(a, "claude");
    s.setAgent(b, "aider");
    s.closePane(a);
    expect([...useLayout.getState().agents]).toEqual([[b, "aider"]]);
  });

  it("the snapshot never carries agents", () => {
    const s = useLayout.getState();
    const id = s.addPane("shell", A)!;
    s.setAgent(id, "claude");
    const json = serializeLayout(useLayout.getState());
    expect(json).not.toContain("agents");
    expect(json).not.toContain("claude");
  });
});
