import { describe, expect, it } from "vitest";
import type { Task } from "./api";
import { columns, forward, isEmpty, today } from "./board";

const veal = { id: 1, name: "veal", path: "C:\\Users\\user\\veal" };
const hungy = { id: 2, name: "hungy", path: "C:\\Users\\user\\hungy" };
const t = (id: string, status: Task["status"]): Task => ({ id, title: id, status, created: "2026-09-17" });

const sources = [
  { project: veal, tasks: [t("v1", "todo"), t("v2", "done"), t("v3", "todo")] },
  { project: hungy, tasks: [t("h1", "doing"), t("h2", "todo")] },
  { project: null, tasks: [t("i1", "todo"), t("i2", "done")] },
];

describe("columns", () => {
  it("preserves source order inside each column and counts per status", () => {
    const cols = columns(sources, "global");
    expect(cols.todo.map((c) => c.task.id)).toEqual(["v1", "v3", "h2", "i1"]);
    expect(cols.doing.map((c) => c.task.id)).toEqual(["h1"]);
    expect(cols.done.map((c) => c.task.id)).toEqual(["v2", "i2"]);
  });

  it("chips project names only in global scope, Inbox always", () => {
    const global = columns(sources, "global");
    expect(global.todo.map((c) => c.chip)).toEqual(["veal", "veal", "hungy", "Inbox"]);
    const project = columns([sources[0], sources[2]], "project");
    expect(project.todo.map((c) => c.chip)).toEqual([null, null, "Inbox"]);
  });

  it("isEmpty is true only when every column is", () => {
    expect(isEmpty(columns([], "project"))).toBe(true);
    expect(isEmpty(columns([{ project: veal, tasks: [] }], "project"))).toBe(true);
    expect(isEmpty(columns([{ project: veal, tasks: [t("x", "done")] }], "project"))).toBe(false);
  });
});

describe("forward", () => {
  it("steps todo → doing → done", () => {
    expect(forward("todo")).toBe("doing");
    expect(forward("doing")).toBe("done");
  });
});

describe("today", () => {
  it("is a local YYYY-MM-DD", () => {
    expect(today(new Date(2026, 8, 7, 23, 59))).toBe("2026-09-07");
    expect(today()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});
