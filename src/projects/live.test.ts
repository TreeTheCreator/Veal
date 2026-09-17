import { describe, expect, it } from "vitest";
import type { Project } from "../db";
import { insert, type LayoutNode, type PaneNode } from "../layout/tree";
import { liveProjectIds, projectForCwd } from "./store";

const projects: Project[] = [
  { id: 1, name: "veal", path: "C:\\Users\\user\\veal" },
  { id: 2, name: "hungy", path: "C:\\Users\\user\\Hungy games" },
];

const pane = (id: number, kind: "shell" | "agent", cwd: string): PaneNode => ({ type: "pane", id, kind, cwd });

describe("liveProjectIds", () => {
  it("lights a project whose root pane has a detected agent, whatever the label", () => {
    let tree: LayoutNode | null = null;
    tree = insert(tree, pane(1, "shell", "C:\\Users\\user\\veal"));
    tree = insert(tree, pane(2, "agent", "c:\\users\\user\\hungy games\\"));
    const agents = new Map([[1, "claude"]]);
    expect([...liveProjectIds(tree, projects, agents)]).toEqual([[1, "claude"]]);
  });

  it("does not light a manual agent pane with nothing running", () => {
    const tree = insert(null, pane(2, "agent", "C:\\Users\\user\\Hungy games"));
    expect(liveProjectIds(tree, projects, new Map()).size).toBe(0);
  });

  it("ignores an agent in a pane that is not at the project root", () => {
    const tree = insert(null, pane(1, "shell", "C:\\Users\\user\\veal\\src"));
    expect(liveProjectIds(tree, projects, new Map([[1, "claude"]])).size).toBe(0);
  });

  it("is empty with no panes", () => {
    expect(liveProjectIds(null, projects, new Map([[9, "claude"]])).size).toBe(0);
  });
});

describe("projectForCwd", () => {
  it("matches the project root case-insensitively and returns null elsewhere", () => {
    expect(projectForCwd(projects, "C:\\USERS\\user\\veal\\")?.name).toBe("veal");
    expect(projectForCwd(projects, "C:\\Users\\user\\veal\\src")).toBeNull();
  });
});
