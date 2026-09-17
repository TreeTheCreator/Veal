import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./api";

const list = vi.fn<(root: string | null) => Promise<Task[]>>();
const add = vi.fn<(root: string | null, title: string, created: string) => Promise<Task[]>>();
const setStatus = vi.fn<(root: string | null, id: string, status: string) => Promise<Task[]>>();
const remove = vi.fn<(root: string | null, id: string) => Promise<Task[]>>();
vi.mock("./api", () => ({
  tasks: {
    list: (r: string | null) => list(r),
    add: (r: string | null, t: string, c: string) => add(r, t, c),
    setStatus: (r: string | null, i: string, s: string) => setStatus(r, i, s),
    remove: (r: string | null, i: string) => remove(r, i),
  },
}));

const { useTasks, selectList, touchesTasks } = await import("./store");

const ROOT = "C:\\Users\\user\\veal";
const t = (id: string, status: Task["status"] = "todo"): Task => ({ id, title: id, status, created: "2026-09-17" });

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  useTasks.setState({ scope: "project", byProject: new Map(), inbox: { status: "loaded", tasks: [] } });
  for (const fn of [list, add, setStatus, remove]) fn.mockReset();
  list.mockResolvedValue([t("a")]);
});

describe("tasks store", () => {
  it("load keeps stale tasks while loading, then replaces them", async () => {
    useTasks.setState({ byProject: new Map([[1, { status: "loaded", tasks: [t("old")] }]]) });
    let resolve!: (v: Task[]) => void;
    list.mockReturnValueOnce(new Promise((r) => (resolve = r)));
    const p = useTasks.getState().load(1, ROOT);
    expect(selectList(1)(useTasks.getState())).toEqual({ status: "loading", tasks: [t("old")] });
    resolve([t("new")]);
    await p;
    expect(selectList(1)(useTasks.getState())).toEqual({ status: "loaded", tasks: [t("new")] });
    expect(list).toHaveBeenCalledWith(ROOT);
  });

  it("add replaces the list with the command's result", async () => {
    add.mockResolvedValue([t("a"), t("b")]);
    const ok = await useTasks.getState().add(1, ROOT, "b", "2026-09-17");
    expect(ok).toBe(true);
    expect(add).toHaveBeenCalledWith(ROOT, "b", "2026-09-17");
    expect(selectList(1)(useTasks.getState()).tasks.map((x) => x.id)).toEqual(["a", "b"]);
    expect(list).not.toHaveBeenCalled();
  });

  it("a rejected mutation records the error, keeps the tasks and reloads", async () => {
    useTasks.setState({ byProject: new Map([[1, { status: "loaded", tasks: [t("a")] }]]) });
    setStatus.mockRejectedValue('tasks.json: expected `,` at line 4 column 7');
    let resolve!: (v: Task[]) => void;
    list.mockReturnValueOnce(new Promise((r) => (resolve = r)));
    const ok = await useTasks.getState().setStatus(1, ROOT, "a", "done");
    expect(ok).toBe(false);
    const s = selectList(1)(useTasks.getState());
    expect(s.status === "error" && s.error).toBe("tasks.json: expected `,` at line 4 column 7");
    expect(s.tasks).toEqual([t("a")]);
    expect(list).toHaveBeenCalledWith(ROOT);
    resolve([t("a")]);
    await flush();
    expect(selectList(1)(useTasks.getState())).toEqual({ status: "loaded", tasks: [t("a")] });
  });

  it("a mutation on a broken file keeps the file's own error after the reload", async () => {
    remove.mockRejectedValue("tasks.json is version 2; this Veal reads version 1");
    list.mockRejectedValue("tasks.json is version 2; this Veal reads version 1");
    await useTasks.getState().remove(1, ROOT, "a");
    await flush();
    const s = selectList(1)(useTasks.getState());
    expect(s.status === "error" && s.error).toBe("tasks.json is version 2; this Veal reads version 1");
  });

  it("forget drops the project", async () => {
    await useTasks.getState().load(1, ROOT);
    useTasks.getState().forget(1);
    expect(useTasks.getState().byProject.has(1)).toBe(false);
    expect(selectList(1)(useTasks.getState())).toEqual({ status: "loaded", tasks: [] });
  });

  it("the Inbox is root === null on every call", async () => {
    add.mockResolvedValue([t("i")]);
    remove.mockResolvedValue([]);
    setStatus.mockResolvedValue([t("i", "done")]);
    const s = useTasks.getState();
    await s.load(null, null);
    await s.add(null, null, "x", "2026-09-17");
    await s.setStatus(null, null, "i", "done");
    expect(useTasks.getState().inbox.tasks).toEqual([t("i", "done")]);
    await s.remove(null, null, "i");
    expect(list).toHaveBeenCalledWith(null);
    expect(add).toHaveBeenCalledWith(null, "x", "2026-09-17");
    expect(setStatus).toHaveBeenCalledWith(null, "i", "done");
    expect(remove).toHaveBeenCalledWith(null, "i");
    expect(useTasks.getState().inbox).toEqual({ status: "loaded", tasks: [] });
  });

  describe("onChanged (watcher)", () => {
    const TASKS = `${ROOT}\\.veal\\tasks.json`;

    it("reloads a loaded project without the loading flip", async () => {
      useTasks.setState({ byProject: new Map([[1, { status: "loaded", tasks: [t("old")] }]]) });
      const seen: string[] = [];
      const unsub = useTasks.subscribe((s) => seen.push(selectList(1)(s).status));
      useTasks.getState().onChanged(1, ROOT, [TASKS]);
      await flush();
      unsub();
      expect(list).toHaveBeenCalledWith(ROOT);
      expect(seen).not.toContain("loading");
      expect(selectList(1)(useTasks.getState())).toEqual({ status: "loaded", tasks: [t("a")] });
    });

    it("ignores a project never loaded, and paths that are not the task file", () => {
      useTasks.getState().onChanged(1, ROOT, [TASKS]);
      useTasks.setState({ byProject: new Map([[1, { status: "loaded", tasks: [] }]]) });
      useTasks.getState().onChanged(1, ROOT, [`${ROOT}\\src\\App.tsx`]);
      expect(list).not.toHaveBeenCalled();
    });

    it("matches the .veal directory itself, any case, and null", async () => {
      useTasks.setState({ byProject: new Map([[1, { status: "loaded", tasks: [] }]]) });
      const s = useTasks.getState();
      s.onChanged(1, ROOT, [`${ROOT}\\.veal`]);
      s.onChanged(1, ROOT, [`c:/users/user/veal/.VEAL/Tasks.json`]);
      s.onChanged(1, ROOT, null);
      await flush();
      expect(list).toHaveBeenCalledTimes(3);
    });
  });

  it("touchesTasks", () => {
    expect(touchesTasks(ROOT, null)).toBe(true);
    expect(touchesTasks(ROOT, [`${ROOT}\\.veal\\tasks.json`])).toBe(true);
    expect(touchesTasks(ROOT, [`${ROOT}\\.veal\\tasks.json.tmp`])).toBe(false);
    expect(touchesTasks(ROOT, [`${ROOT}\\x`])).toBe(false);
  });
});
