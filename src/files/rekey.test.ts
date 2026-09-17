import { describe, expect, it } from "vitest";
import { dropUnder, isUnder, rekeyProject } from "./rekey";
import type { ProjectFiles } from "./store";
import { pathKey, type DirState } from "./tree";

const A = "C:\\Users\\user\\veal";
const loaded = (names: string[]): DirState => ({ status: "loaded", entries: names.map((name) => ({ name, kind: "file", size: 1 })) });

/** Root with `src` (expanded) and `src\files`; `src\App.tsx` open and dirty; `src\files\x.ts` in edit mode. */
function project(): ProjectFiles {
  return {
    root: A,
    dirs: new Map([
      [pathKey(A), loaded(["x.md"])],
      [pathKey(`${A}\\src`), loaded(["App.tsx"])],
      [pathKey(`${A}\\src\\files`), loaded(["x.ts"])],
    ]),
    expanded: new Set([pathKey(`${A}\\src`), pathKey(`${A}\\src\\files`)]),
    open: `${A}\\src\\App.tsx`,
    file: { status: "loading" },
    editing: new Set([pathKey(`${A}\\src\\App.tsx`), pathKey(`${A}\\src\\files\\x.ts`)]),
    drafts: new Map([[pathKey(`${A}\\src\\App.tsx`), { state: null, mtime: 1, path: `${A}\\src\\App.tsx` }]]),
  };
}

describe("isUnder", () => {
  it("matches the key itself and descendants, not siblings with the same prefix", () => {
    const k = pathKey(`${A}\\src`);
    expect(isUnder(k, k)).toBe(true);
    expect(isUnder(pathKey(`${A}\\src\\a.ts`), k)).toBe(true);
    expect(isUnder(pathKey(`${A}\\src-tauri\\a.rs`), k)).toBe(false);
    expect(isUnder(pathKey(A), k)).toBe(false);
  });
});

describe("rekeyProject", () => {
  it("renaming a directory moves every key under it and the open path", () => {
    const p = rekeyProject(project(), `${A}\\src`, `${A}\\lib`);
    expect([...p.dirs.keys()].sort()).toEqual([pathKey(A), pathKey(`${A}\\lib`), pathKey(`${A}\\lib\\files`)].sort());
    expect([...p.expanded].sort()).toEqual([pathKey(`${A}\\lib`), pathKey(`${A}\\lib\\files`)].sort());
    expect([...p.editing].sort()).toEqual([pathKey(`${A}\\lib\\App.tsx`), pathKey(`${A}\\lib\\files\\x.ts`)].sort());
    expect(p.open).toBe(`${A}\\lib\\App.tsx`);
    const draft = p.drafts.get(pathKey(`${A}\\lib\\App.tsx`));
    expect(draft).toEqual({ state: null, mtime: 1, path: `${A}\\lib\\App.tsx` });
    expect(p.file).toEqual({ status: "loading" });
  });

  it("renaming a file re-keys only that file", () => {
    const p = rekeyProject(project(), `${A}\\src\\App.tsx`, `${A}\\src\\Main.tsx`);
    expect(p.open).toBe(`${A}\\src\\Main.tsx`);
    expect(p.drafts.has(pathKey(`${A}\\src\\Main.tsx`))).toBe(true);
    expect(p.drafts.has(pathKey(`${A}\\src\\App.tsx`))).toBe(false);
    expect(p.editing.has(pathKey(`${A}\\src\\Main.tsx`))).toBe(true);
    expect(p.editing.has(pathKey(`${A}\\src\\files\\x.ts`))).toBe(true);
    expect(p.dirs.size).toBe(3);
  });

  it("an unrelated rename changes nothing", () => {
    const before = project();
    const p = rekeyProject(before, `${A}\\x.md`, `${A}\\y.md`);
    expect(p.open).toBe(before.open);
    expect([...p.dirs.keys()]).toEqual([...before.dirs.keys()]);
    expect(p.drafts).toEqual(before.drafts);
  });
});

describe("dropUnder", () => {
  it("deleting a directory drops its keys and closes the open file inside", () => {
    const p = dropUnder(project(), `${A}\\src`);
    expect([...p.dirs.keys()]).toEqual([pathKey(A)]);
    expect(p.expanded.size).toBe(0);
    expect(p.editing.size).toBe(0);
    expect(p.drafts.size).toBe(0);
    expect(p.open).toBeNull();
    expect(p.file).toBeNull();
  });

  it("deleting another file leaves the open one alone", () => {
    const p = dropUnder(project(), `${A}\\src\\files\\x.ts`);
    expect(p.open).toBe(`${A}\\src\\App.tsx`);
    expect(p.editing.has(pathKey(`${A}\\src\\files\\x.ts`))).toBe(false);
    expect(p.editing.has(pathKey(`${A}\\src\\App.tsx`))).toBe(true);
  });
});
