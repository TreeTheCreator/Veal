import { beforeEach, describe, expect, it, vi } from "vitest";

import { EditorState } from "@codemirror/state";

const list = vi.fn<(dir: string) => Promise<unknown>>();
const read = vi.fn<(path: string) => Promise<unknown>>();
const write = vi.fn<(path: string, text: string, o: unknown) => Promise<unknown>>();
vi.mock("../fs", () => ({
  fs: { list: (d: string) => list(d), read: (p: string) => read(p), write: (p: string, t: string, o: unknown) => write(p, t, o) },
}));
const showToast = vi.fn();
vi.mock("../ui", () => ({ useUi: { getState: () => ({ showToast }) } }));

const { useFiles, detectCrlf } = await import("./store");
const { pathKey } = await import("./tree");

const LOADED = { status: "loaded", text: "hi\n", size: 3, mtime: 1000, bom: false, crlf: false, saving: false, conflict: null, gen: expect.any(Number) };

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
  read.mockImplementation(async () => ({ text: "hi\n", size: 3, mtime: 1000, bom: false }));
  write.mockReset();
  showToast.mockReset();
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
    expect(one.file).toEqual(LOADED);
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
    resolveFirst({ text: "slow", size: 4, mtime: 1, bom: false });
    await flush();
    const p = useFiles.getState().byProject.get(1)!;
    expect(p.open).toBe(`${A}\\x.md`);
    expect(p.file).toEqual(LOADED);
  });
});

describe("editing and drafts", () => {
  const F = `${A}\\x.md`;
  const key = pathKey(F);
  const project = () => useFiles.getState().byProject.get(1)!;
  const loadedFile = () => {
    const f = project().file;
    if (f?.status !== "loaded") throw new Error(`not loaded: ${f?.status}`);
    return f;
  };

  const openAndEdit = async () => {
    const s = useFiles.getState();
    s.ensure(1, A);
    s.openFile(1, F);
    await flush();
    s.setEditing(1, F, true);
    s.markDirty(1, F);
  };

  it("detectCrlf", () => {
    expect(detectCrlf("a\r\nb")).toBe(true);
    expect(detectCrlf("a\nb")).toBe(false);
    expect(detectCrlf("")).toBe(false);
  });

  it("markDirty needs edit mode; setEditing(off) is ignored while dirty", async () => {
    const s = useFiles.getState();
    s.ensure(1, A);
    s.openFile(1, F);
    await flush();
    s.markDirty(1, F);
    expect(project().drafts.size).toBe(0);
    s.setEditing(1, F, true);
    s.markDirty(1, F);
    s.markDirty(1, F);
    expect(project().drafts.get(key)).toEqual({ state: null, mtime: 1000, path: F });
    s.setEditing(1, F, false);
    expect(project().editing.has(key)).toBe(true);
  });

  it("a re-read with the same mtime keeps the draft and text; a newer one flags a conflict", async () => {
    await openAndEdit();
    const gen = loadedFile().gen;
    useFiles.getState().refresh(1);
    await flush();
    expect(loadedFile()).toMatchObject({ text: "hi\n", mtime: 1000, conflict: null, gen });
    expect(project().drafts.has(key)).toBe(true);

    read.mockImplementation(async () => ({ text: "changed\n", size: 8, mtime: 2000, bom: false }));
    useFiles.getState().refresh(1);
    await flush();
    expect(loadedFile()).toMatchObject({ text: "hi\n", mtime: 1000, conflict: "changed", gen });

    read.mockImplementation(async () => {
      throw "gone";
    });
    useFiles.getState().refresh(1);
    await flush();
    expect(loadedFile()).toMatchObject({ text: "hi\n", conflict: "deleted", gen });
  });

  it("stash keeps the EditorState and a switch back restores the loaded shape with a conflict check", async () => {
    await openAndEdit();
    const s = useFiles.getState();
    const state = EditorState.create({ doc: "draft" });
    s.stash(1, F, state);
    s.openFile(1, `${A}\\other.md`);
    await flush();
    read.mockImplementation(async () => ({ text: "hi\n", size: 3, mtime: 3000, bom: false }));
    s.openFile(1, F);
    await flush();
    expect(project().drafts.get(key)).toEqual({ state, mtime: 1000, path: F });
    expect(loadedFile()).toMatchObject({ mtime: 3000, conflict: "changed" });
  });

  it("save writes with the loaded mtime, crlf and bom, then drops the draft and edit mode", async () => {
    read.mockImplementation(async () => ({ text: "a\r\nb", size: 4, mtime: 1000, bom: true }));
    await openAndEdit();
    write.mockResolvedValue({ size: 9, mtime: 5000 });
    const gen = loadedFile().gen;
    const ok = await useFiles.getState().save(1, F, "a\nb\nc");
    expect(ok).toBe(true);
    expect(write).toHaveBeenCalledWith(F, "a\nb\nc", { crlf: true, bom: true, expectedMtime: 1000 });
    expect(loadedFile()).toMatchObject({ text: "a\r\nb", size: 9, mtime: 5000, saving: false, conflict: null, gen });
    expect(project().drafts.size).toBe(0);
    expect(project().editing.size).toBe(0);
  });

  it("a stale save sets conflict and keeps the draft; force skips the check", async () => {
    await openAndEdit();
    write.mockRejectedValueOnce("changed on disk");
    expect(await useFiles.getState().save(1, F, "x")).toBe(false);
    expect(loadedFile()).toMatchObject({ saving: false, conflict: "changed" });
    expect(project().drafts.has(key)).toBe(true);
    expect(showToast).not.toHaveBeenCalled();

    write.mockResolvedValueOnce({ size: 1, mtime: 7000 });
    expect(await useFiles.getState().save(1, F, "x", true)).toBe(true);
    expect(write).toHaveBeenLastCalledWith(F, "x", { crlf: false, bom: false, expectedMtime: null });
    expect(loadedFile()).toMatchObject({ mtime: 7000, conflict: null });
  });

  it("other save errors toast and keep everything", async () => {
    await openAndEdit();
    write.mockRejectedValueOnce("write C:\\x: access denied");
    expect(await useFiles.getState().save(1, F, "x")).toBe(false);
    expect(showToast).toHaveBeenCalledWith("Save failed: write C:\\x: access denied");
    expect(loadedFile()).toMatchObject({ saving: false, conflict: null });
    expect(project().drafts.has(key)).toBe(true);
  });

  it("discard drops the draft and edit mode and re-reads with a new gen", async () => {
    await openAndEdit();
    const gen = loadedFile().gen;
    useFiles.getState().discard(1, F);
    await flush();
    expect(project().drafts.size).toBe(0);
    expect(project().editing.size).toBe(0);
    expect(loadedFile().gen).not.toBe(gen);
  });

  it("dirtyFiles spans projects and forget drops them", async () => {
    await openAndEdit();
    const s = useFiles.getState();
    s.ensure(2, B);
    s.openFile(2, `${B}\\y.md`);
    await flush();
    s.setEditing(2, `${B}\\y.md`, true);
    s.markDirty(2, `${B}\\y.md`);
    expect(s.dirtyFiles()).toEqual([
      { projectId: 1, root: A, path: F },
      { projectId: 2, root: B, path: `${B}\\y.md` },
    ]);
    s.forget(1);
    expect(s.dirtyFiles()).toEqual([{ projectId: 2, root: B, path: `${B}\\y.md` }]);
  });
});
