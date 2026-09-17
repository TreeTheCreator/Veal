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
  it("lights only projects with an agent pane at their root", () => {
    let tree: LayoutNode | null = null;
    tree = insert(tree, pane(1, "shell", "C:\\Users\\user\\veal"));
    tree = insert(tree, pane(2, "agent", "c:\\users\\user\\hungy games\\"));
    expect([...liveProjectIds(tree, projects)]).toEqual([2]);
  });
  it("is empty with no panes", () => {
    expect(liveProjectIds(null, projects).size).toBe(0);
  });
});

describe("projectForCwd", () => {
  it("matches the project root case-insensitively and returns null elsewhere", () => {
    expect(projectForCwd(projects, "C:\\USERS\\user\\veal\\")?.name).toBe("veal");
    expect(projectForCwd(projects, "C:\\Users\\user\\veal\\src")).toBeNull();
  });
});
