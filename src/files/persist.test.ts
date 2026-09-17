import { describe, expect, it, vi } from "vitest";

vi.mock("../db", () => ({ db: { settingsSet: async () => {} } }));
const { filesKey, parseAllFiles, parseFiles, serializeFiles } = await import("./persist");

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
