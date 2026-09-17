import { describe, expect, it, vi } from "vitest";

vi.mock("../db", () => ({ db: { settingsSet: async () => {} } }));
const { filesKey, parseAllFiles, parseFiles, serializeFiles, settledExpanded } = await import("./persist");
const { pathKey } = await import("./tree");

const A = "C:\\Users\\user\\veal";

describe("files persistence (plan-11)", () => {
  it("round-trips", () => {
    const s = { expanded: [`${A}\\src`, `${A}\\src\\files`], open: `${A}\\src\\App.tsx` };
    expect(parseFiles(serializeFiles(s))).toEqual(s);
    expect(parseFiles(serializeFiles({ expanded: [], open: null }))).toEqual({ expanded: [], open: null });
  });

  it("rejects garbage and treats empty as nothing", () => {
    for (const bad of ["", "nope", "[]", "{}", '{"expanded":"x","open":null}', '{"expanded":[1],"open":null}', '{"expanded":[],"open":""}']) {
      expect(parseFiles(bad), bad).toBeNull();
    }
    expect(parseFiles(undefined)).toBeNull();
  });

  describe("settledExpanded (plan-12)", () => {
    type Dirs = Parameters<typeof settledExpanded>[0]["dirs"];
    const src = `${A}\\src`;
    const files = `${src}\\files`;
    const dir = (name: string) => ({ name, kind: "dir" as const, size: 0 });
    const expanded = new Set([pathKey(src), pathKey(files)]);

    it("is null while the tree cannot be flattened yet", () => {
      // Right after a restore: expanded is seeded, every listing in flight.
      const dirs: Dirs = new Map([[pathKey(A), { status: "loading" }], [pathKey(src), { status: "loading" }]]);
      expect(settledExpanded({ root: A, dirs, expanded })).toBeNull();
      // The root arrived, src has not: files is still unreachable.
      dirs.set(pathKey(A), { status: "loaded", entries: [dir("src")] });
      expect(settledExpanded({ root: A, dirs, expanded })).toBeNull();
    });

    it("gives the real paths once every listing has settled", () => {
      const dirs: Dirs = new Map([
        [pathKey(A), { status: "loaded", entries: [dir("src")] }],
        [pathKey(src), { status: "loaded", entries: [dir("files")] }],
        [pathKey(files), { status: "loaded", entries: [] }],
      ]);
      expect(settledExpanded({ root: A, dirs, expanded })).toEqual([src, files]);
    });

    it("drops a directory that is gone once nothing is loading", () => {
      const dirs: Dirs = new Map([
        [pathKey(A), { status: "loaded", entries: [dir("src")] }],
        [pathKey(src), { status: "loaded", entries: [] }], // `files` was deleted
      ]);
      expect(settledExpanded({ root: A, dirs, expanded })).toEqual([src]);
      // A listing that failed is settled too.
      dirs.set(pathKey(A), { status: "error", error: "gone" });
      expect(settledExpanded({ root: A, dirs, expanded })).toEqual([]);
    });
  });

  it("collects every files.<id> key from the settings map", () => {
    const map = {
      layout: "{}",
      [filesKey(1)]: serializeFiles({ expanded: [A], open: null }),
      [filesKey(3)]: "",
      "files.x": serializeFiles({ expanded: [], open: null }),
    };
    expect([...parseAllFiles(map).keys()]).toEqual([1]);
  });
});
