import { beforeEach, describe, expect, it, vi } from "vitest";

const list = vi.fn<(dir: string) => Promise<unknown>>();
const read = vi.fn<(path: string) => Promise<unknown>>();
vi.mock("../fs", () => ({ fs: { list: (d: string) => list(d), read: (p: string) => read(p) } }));

const { useFiles } = await import("./store");
const { pathKey } = await import("./tree");

const A = "C:\\Users\\user\\veal";
const B = "C:\\Users\\user\\hungy";
const dir = (name: string) => ({ name, kind: "dir", size: 0 });
const file = (name: string) => ({ name, kind: "file", size: 3 });

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  useFiles.setState({ byProject: new Map() });
  list.mockReset();
  read.mockReset();
  list.mockImplementation(async (d) => (d.endsWith("src") ? [file("App.tsx")] : [dir("src"), file("x.md")]));
  read.mockImplementation(async () => ({ text: "hi\n", size: 3 }));
});

describe("files store", () => {
  it("lists the root once on ensure", async () => {
    useFiles.getState().ensure(1, A);
    useFiles.getState().ensure(1, A);
    await flush();
    expect(list).toHaveBeenCalledTimes(1);
    expect(list).toHaveBeenCalledWith(A);
    expect(useFiles.getState().byProject.get(1)?.dirs.get(pathKey(A))?.status).toBe("loaded");
  });

  it("keeps an open file and expanded dirs per project across a switch", async () => {
    const s = useFiles.getState();
    s.ensure(1, A);
    s.ensure(2, B);
    s.toggleDir(1, `${A}\\src`);
    s.openFile(1, `${A}\\src\\App.tsx`);
    await flush();

    const one = useFiles.getState().byProject.get(1)!;
    expect(one.open).toBe(`${A}\\src\\App.tsx`);
    expect(one.file).toEqual({ status: "loaded", text: "hi\n", size: 3 });
    expect(one.expanded.has(pathKey(`${A}\\src`))).toBe(true);

    const two = useFiles.getState().byProject.get(2)!;
    expect(two.open).toBeNull();
    expect(two.expanded.size).toBe(0);

    useFiles.getState().forget(1);
    expect(useFiles.getState().byProject.has(1)).toBe(false);
    expect(useFiles.getState().byProject.has(2)).toBe(true);
  });

  it("collapsing keeps the cached listing; expanding re-lists", async () => {
    const s = useFiles.getState();
    s.ensure(1, A);
    const src = `${A}\\src`;
    s.toggleDir(1, src);
    await flush();
    expect(list).toHaveBeenCalledTimes(2);

    s.toggleDir(1, src); // collapse
    expect(list).toHaveBeenCalledTimes(2);
    expect(useFiles.getState().byProject.get(1)?.dirs.get(pathKey(src))?.entries).toEqual([file("App.tsx")]);

    s.toggleDir(1, src); // expand again → re-list, stale entries shown meanwhile
    expect(useFiles.getState().byProject.get(1)?.dirs.get(pathKey(src))).toEqual({
      status: "loading",
      entries: [file("App.tsx")],
    });
    await flush();
    expect(list).toHaveBeenCalledTimes(3);
  });

  it("refresh re-lists the root and expanded dirs only, and re-reads the open file", async () => {
    const s = useFiles.getState();
    s.ensure(1, A);
    s.toggleDir(1, `${A}\\src`);
    s.toggleDir(1, `${A}\\src`); // collapsed again: listed but not expanded
    s.openFile(1, `${A}\\x.md`);
    await flush();
    list.mockClear();
    read.mockClear();

    useFiles.getState().refresh(1);
    await flush();
    expect(list.mock.calls.map((c) => c[0])).toEqual([A]);
    expect(read).toHaveBeenCalledWith(`${A}\\x.md`);
  });

  it("records a listing error and a read error", async () => {
    list.mockImplementation(async () => {
      throw "C:\\nope: not found";
    });
    read.mockImplementation(async () => {
      throw "binary";
    });
    const s = useFiles.getState();
    s.ensure(1, A);
    s.openFile(1, `${A}\\a.bin`);
    await flush();
    const p = useFiles.getState().byProject.get(1)!;
    expect(p.dirs.get(pathKey(A))).toEqual({ status: "error", error: "C:\\nope: not found", entries: undefined });
    expect(p.file).toEqual({ status: "error", error: "binary" });
  });

  it("ignores a read that finishes after another file was opened", async () => {
    let resolveFirst!: (v: unknown) => void;
    read.mockImplementationOnce(() => new Promise((r) => (resolveFirst = r)));
    const s = useFiles.getState();
    s.ensure(1, A);
    s.openFile(1, `${A}\\slow.md`);
    s.openFile(1, `${A}\\x.md`);
    await flush();
    resolveFirst({ text: "slow", size: 4 });
    await flush();
    const p = useFiles.getState().byProject.get(1)!;
    expect(p.open).toBe(`${A}\\x.md`);
    expect(p.file).toEqual({ status: "loaded", text: "hi\n", size: 3 });
  });
});
