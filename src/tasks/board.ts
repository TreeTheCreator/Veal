// Pure board logic: which cards go in which column, and what chip each
// shows. Sources arrive in display order (sidebar order, Inbox last).
import type { Project } from "../db";
import { STATUSES, type Status, type Task } from "./api";

export type Scope = "project" | "global";

export interface Source {
  /** `null` is the Inbox. */
  project: Project | null;
  tasks: Task[];
}

export interface Card {
  task: Task;
  project: Project | null;
  /** Project name in global scope, `Inbox` for Inbox tasks in either scope, else none. */
  chip: string | null;
}

export type Columns = Record<Status, Card[]>;

export const COLUMN_LABEL: Record<Status, string> = { todo: "To do", doing: "In progress", done: "Done" };

export function columns(sources: Source[], scope: Scope): Columns {
  const out: Columns = { todo: [], doing: [], done: [] };
  for (const { project, tasks } of sources) {
    const chip = project === null ? "Inbox" : scope === "global" ? project.name : null;
    for (const task of tasks) out[task.status].push({ task, project, chip });
  }
  return out;
}

export function isEmpty(cols: Columns): boolean {
  return STATUSES.every((s) => cols[s].length === 0);
}

/** ▸: todo → doing → done. */
export function forward(status: Status): Status {
  return status === "todo" ? "doing" : "done";
}

/** ◂: done → doing → todo (plan-11). */
export function backward(status: Status): Status {
  return status === "done" ? "doing" : "todo";
}

/** Local `YYYY-MM-DD` — the webview is the only clock that knows the zone. */
export function today(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
