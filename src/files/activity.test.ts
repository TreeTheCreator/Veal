import { describe, expect, it } from "vitest";
import { counts, lit, nextExpiry, OWN_MS, prune, record, settle, TOUCH_MS, type Touches } from "./activity";
import { pathKey } from "./tree";

const ROOT = "C:\\Users\\user\\veal";
const at = (rel: string) => `${ROOT}\\${rel}`;
const none: Touches = new Map();

describe("counts", () => {
  it("takes paths under the root", () => {
    expect(counts(ROOT, at("src\\App.tsx"))).toBe(true);
    expect(counts(ROOT + "\\", at("README.md"))).toBe(true);
  });

  it("skips paths outside the root, and the root itself", () => {
    expect(counts(ROOT, "C:\\Users\\user\\other\\a.ts")).toBe(false);
    expect(counts(ROOT, "C:\\Users\\user\\vealish\\a.ts")).toBe(false);
    expect(counts(ROOT, ROOT)).toBe(false);
  });

  it("skips build output and Veal's own folder", () => {
    expect(counts(ROOT, at("node_modules\\react\\index.js"))).toBe(false);
    expect(counts(ROOT, at("dist\\index.html"))).toBe(false);
    expect(counts(ROOT, at("src-tauri\\target\\debug\\veal.exe"))).toBe(false);
    expect(counts(ROOT, at(".veal\\tasks.json"))).toBe(false);
  });
});

describe("record", () => {
  it("keeps the latest path lit with no expiry, and fades the rest of the batch", () => {
    const m = record(none, ROOT, [at("src\\App.tsx"), at("src\\ui.ts")], none, 1000);
    expect(m.get(pathKey(at("src\\App.tsx")))).toBe(1000 + TOUCH_MS);
    expect(m.get(pathKey(at("src\\ui.ts")))).toBe(Infinity);
  });

  it("starts fading the previous latest when the agent moves on", () => {
    const first = record(none, ROOT, [at("a.ts")], none, 1000);
    const m = record(first, ROOT, [at("b.ts")], none, 5000);
    expect(m.get(pathKey(at("a.ts")))).toBe(5000 + TOUCH_MS);
    expect(m.get(pathKey(at("b.ts")))).toBe(Infinity);
  });

  it("keeps the latest when a batch counts nothing", () => {
    const first = record(none, ROOT, [at("a.ts")], none, 1000);
    const m = record(first, ROOT, [at("dist\\a.js")], none, 5000);
    expect(m.get(pathKey(at("a.ts")))).toBe(Infinity);
  });

  it("skips our own recent writes", () => {
    const own: Touches = new Map([[pathKey(at("src\\App.tsx")), 1000 + OWN_MS]]);
    const m = record(none, ROOT, [at("src\\App.tsx"), at("src\\ui.ts")], own, 1000);
    expect([...m]).toEqual([[pathKey(at("src\\ui.ts")), Infinity]]);
  });

  it("counts our own write again once its window has passed", () => {
    const own: Touches = new Map([[pathKey(at("a.ts")), 1000]]);
    expect(record(none, ROOT, [at("a.ts")], own, 1000).size).toBe(1);
  });

  it("drops expired entries as it records", () => {
    const old: Touches = new Map([[pathKey(at("old.ts")), 500]]);
    expect([...record(old, ROOT, [at("new.ts")], none, 1000).keys()]).toEqual([pathKey(at("new.ts"))]);
  });
});

describe("settle", () => {
  it("starts the latest fading and leaves the rest", () => {
    const m = settle(new Map([["a", Infinity], ["b", 2000]]), 1000);
    expect([...m]).toEqual([["a", 1000 + TOUCH_MS], ["b", 2000]]);
  });

  it("returns the same map when there is no latest", () => {
    const m: Touches = new Map([["a", 2000]]);
    expect(settle(m, 1000)).toBe(m);
  });
});

describe("prune", () => {
  it("never drops the latest", () => {
    expect(prune(new Map([["a", Infinity]]), 1e15).size).toBe(1);
  });

  it("returns the same map when nothing expired", () => {
    const m: Touches = new Map([["a", 2000]]);
    expect(prune(m, 1000)).toBe(m);
  });

  it("drops what expired", () => {
    expect([...prune(new Map([["a", 1000], ["b", 2000]]), 1000).keys()]).toEqual(["b"]);
  });
});

describe("lit", () => {
  it("lights the file and every folder up to, not including, the root", () => {
    const m = record(none, ROOT, [at("src\\components\\FileTree.tsx")], none, 0);
    const { paths, dirs } = lit(m, ROOT, 1);
    expect([...paths]).toEqual([pathKey(at("src\\components\\FileTree.tsx"))]);
    expect([...dirs].sort()).toEqual([pathKey(at("src")), pathKey(at("src\\components"))]);
  });

  it("lights nothing for a file at the root but the file", () => {
    const { dirs } = lit(record(none, ROOT, [at("README.md")], none, 0), ROOT, 1);
    expect(dirs.size).toBe(0);
  });

  it("ignores expired entries and a missing map", () => {
    expect(lit(new Map([[pathKey(at("a.ts")), 5]]), ROOT, 5).paths.size).toBe(0);
    expect(lit(undefined, ROOT, 0).paths.size).toBe(0);
  });
});

describe("nextExpiry", () => {
  it("skips the latest, which never expires", () => {
    expect(nextExpiry([new Map([["a", Infinity]])])).toBeNull();
  });

  it("finds the soonest across projects", () => {
    expect(nextExpiry([new Map([["a", 30]]), new Map([["b", 10], ["c", 20]])])).toBe(10);
    expect(nextExpiry([])).toBeNull();
  });
});
