import { describe, expect, it } from "vitest";
import type { Entry } from "../fs";
import { flatten, formatSize, parentIndex, pathKey, type DirState } from "./tree";

const ROOT = "C:\\Users\\user\\veal";
const dir = (name: string): Entry => ({ name, kind: "dir", size: 0 });
const file = (name: string, size = 10): Entry => ({ name, kind: "file", size });

const loaded = (entries: Entry[]): DirState => ({ status: "loaded", entries });

function dirs(map: Record<string, DirState>): Map<string, DirState> {
  return new Map(Object.entries(map).map(([k, v]) => [pathKey(k), v]));
}
const expanded = (...paths: string[]) => new Set(paths.map(pathKey));

const rootListing = loaded([dir("src"), dir("src-tauri"), file("package.json", 823)]);

describe("flatten", () => {
  it("emits only the root rows when nothing is expanded", () => {
    const rows = flatten(ROOT, dirs({ [ROOT]: rootListing }), expanded());
    expect(rows.map((r) => [r.name, r.depth, r.kind])).toEqual([
      ["src", 0, "dir"],
      ["src-tauri", 0, "dir"],
      ["package.json", 0, "file"],
    ]);
    expect(rows[0].path).toBe(`${ROOT}\\src`);
    expect(rows[0].expanded).toBe(false);
  });

  it("inserts an expanded directory's rows right after it at depth + 1", () => {
    const rows = flatten(
      ROOT,
      dirs({ [ROOT]: rootListing, [`${ROOT}\\src`]: loaded([dir("layout"), file("App.tsx")]) }),
      expanded(`${ROOT}\\src`),
    );
    expect(rows.map((r) => `${r.depth}:${r.name}`)).toEqual([
      "0:src", "1:layout", "1:App.tsx", "0:src-tauri", "0:package.json",
    ]);
    expect(rows[0].expanded).toBe(true);
    expect(rows[0].state).toBe("loaded");
  });

  it("shows an expanded directory that is still loading with no children", () => {
    const rows = flatten(ROOT, dirs({ [ROOT]: rootListing }), expanded(`${ROOT}\\src`));
    expect(rows.map((r) => r.name)).toEqual(["src", "src-tauri", "package.json"]);
    expect(rows[0].state).toBe("loading");
  });

  it("keeps stale children visible while a re-list is in flight, and surfaces errors", () => {
    const src = `${ROOT}\\src`;
    const rows = flatten(
      ROOT,
      dirs({ [ROOT]: rootListing, [src]: { status: "loading", entries: [file("old.ts")] } }),
      expanded(src),
    );
    expect(rows.map((r) => r.name)).toEqual(["src", "old.ts", "src-tauri", "package.json"]);

    const failed = flatten(
      ROOT,
      dirs({ [ROOT]: rootListing, [src]: { status: "error", error: "denied" } }),
      expanded(src),
    );
    expect(failed[0].error).toBe("denied");
  });

  it("hides descendants of a collapsed directory but a nested expansion survives re-expanding", () => {
    const src = `${ROOT}\\src`;
    const layout = `${src}\\layout`;
    const d = dirs({
      [ROOT]: rootListing,
      [src]: loaded([dir("layout")]),
      [layout]: loaded([file("tree.ts")]),
    });
    const both = flatten(ROOT, d, expanded(src, layout));
    expect(both.map((r) => `${r.depth}:${r.name}`)).toContain("2:tree.ts");

    const collapsed = flatten(ROOT, d, expanded(layout));
    expect(collapsed.map((r) => r.name)).not.toContain("layout");
    expect(collapsed.map((r) => r.name)).not.toContain("tree.ts");
  });

  it("keys paths case-insensitively with either separator", () => {
    const rows = flatten("c:/users/user/VEAL", dirs({ [ROOT]: rootListing }), expanded());
    expect(rows).toHaveLength(3);
  });
});

describe("parentIndex", () => {
  it("finds the nearest shallower row above", () => {
    const rows = flatten(
      ROOT,
      dirs({ [ROOT]: rootListing, [`${ROOT}\\src`]: loaded([dir("layout"), file("App.tsx")]) }),
      expanded(`${ROOT}\\src`),
    );
    expect(parentIndex(rows, 2)).toBe(0); // App.tsx → src
    expect(parentIndex(rows, 3)).toBe(-1); // src-tauri is top level
  });
});

describe("formatSize", () => {
  it("picks B / KB / MB", () => {
    expect(formatSize(823)).toBe("823 B");
    expect(formatSize(5_734)).toBe("5.6 KB");
    expect(formatSize(4_300_000)).toBe("4.1 MB");
  });
});

describe("relativeTo", () => {
  it("strips the root and uses forward slashes", async () => {
    const { relativeTo } = await import("../components/FilesView");
    expect(relativeTo(ROOT, `${ROOT}\\src\\App.tsx`)).toBe("src/App.tsx");
    expect(relativeTo("c:/users/user/veal/", "C:\\Users\\user\\VEAL\\x.md")).toBe("x.md");
    expect(relativeTo(ROOT, ROOT)).toBe("");
    // A sibling that merely shares the prefix is not under the root (plan-14).
    expect(relativeTo(ROOT, `${ROOT}2\\x.md`)).toBe(`${ROOT}2\\x.md`.replace(/\\/g, "/"));
    expect(relativeTo(ROOT, "D:\\elsewhere\\x.md")).toBe("D:/elsewhere/x.md");
  });
});
