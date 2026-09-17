import { describe, expect, it } from "vitest";
import type { ProjectFiles } from "./store";
import { pathKey, type DirState } from "./tree";
import { affected, parentPath } from "./watch";

const A = "C:\\Users\\user\\veal";
const dir = (name: string) => ({ name, kind: "dir" as const, size: 0 });
const file = (name: string) => ({ name, kind: "file" as const, size: 3 });
const loaded = (entries: DirState["entries"]): DirState => ({ status: "loaded", entries: entries ?? [] });

/** Root listed with `src` (expanded, listed) and `src-tauri` (collapsed, never listed); `src\\App.tsx` open. */
function project(over: Partial<ProjectFiles> = {}): ProjectFiles {
  return {
    root: A,
    dirs: new Map([
      [pathKey(A), loaded([dir("src"), dir("src-tauri"), file("x.md")])],
      [pathKey(`${A}\\src`), loaded([file("App.tsx"), dir("files")])],
    ]),
    expanded: new Set([pathKey(`${A}\\src`)]),
    open: `${A}\\src\\App.tsx`,
    file: null,
    editing: new Set(),
    drafts: new Map(),
    ...over,
  };
}

describe("parentPath", () => {
  it("strips the last segment, either separator", () => {
    expect(parentPath("C:\\a\\b\\c")).toBe("C:\\a\\b");
    expect(parentPath("C:/a/b/c")).toBe("C:/a/b");
    expect(parentPath("C:\\a\\b\\")).toBe("C:\\a");
    expect(parentPath("C:\\a")).toBe("C:\\");
    expect(parentPath("C:\\")).toBeNull();
  });
});

describe("affected", () => {
  it("re-lists an expanded parent, not a collapsed one", () => {
    expect(affected(project(), [`${A}\\src\\new.ts`])).toEqual({ dirs: [`${A}\\src`], file: false });
    expect(affected(project(), [`${A}\\src-tauri\\Cargo.toml`])).toEqual({ dirs: [], file: false });
    expect(affected(project(), [`${A}\\src\\files\\deep.ts`])).toEqual({ dirs: [], file: false });
  });

  it("re-lists the root for a top-level path", () => {
    expect(affected(project(), [`${A}\\y.md`])).toEqual({ dirs: [A], file: false });
  });

  it("flags the open file and re-lists its parent", () => {
    expect(affected(project(), [`${A}\\src\\App.tsx`])).toEqual({ dirs: [`${A}\\src`], file: true });
  });

  it("re-lists an expanded directory named directly, plus its parent", () => {
    expect(affected(project(), [`${A}\\src`])).toEqual({ dirs: [`${A}\\src`, A], file: false });
  });

  it("deduplicates and matches case-insensitively with either separator", () => {
    const out = affected(project(), [`${A}\\src\\a.ts`, `${A}\\src\\b.ts`, `c:/users/USER/veal/SRC/App.TSX`]);
    expect(out).toEqual({ dirs: [`${A}\\src`], file: true });
  });

  it("ignores an expanded directory whose listing never arrived", () => {
    const p = project({ expanded: new Set([pathKey(`${A}\\src`), pathKey(`${A}\\src-tauri`)]) });
    expect(affected(p, [`${A}\\src-tauri\\Cargo.toml`])).toEqual({ dirs: [], file: false });
  });
});
