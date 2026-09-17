import { describe, expect, it } from "vitest";
import { basename, deriveProject, joinPath, normalizePath, samePath, tilde } from "./paths";

const HOME = "C:\\Users\\user";

describe("samePath", () => {
  it("ignores case, separator style and trailing separators", () => {
    expect(samePath("C:\\Users\\user\\veal\\", "c:/users/user/veal")).toBe(true);
    expect(samePath("C:\\Users\\user\\veal", "C:\\Users\\user\\veal2")).toBe(false);
  });
  it("keeps a bare drive root and strips repeated trailing separators", () => {
    expect(normalizePath("C:\\")).toBe("C:\\");
    expect(normalizePath("C:/")).toBe("C:\\");
    expect(normalizePath("  C:\\x\\\\ ")).toBe("C:\\x");
  });
  it("canonicalises forward slashes only in drive-letter paths", () => {
    expect(normalizePath("C:/Users/user/veal/")).toBe("C:\\Users\\user\\veal");
    expect(normalizePath("/home/u/veal/")).toBe("/home/u/veal");
  });
});

describe("basename / tilde", () => {
  it("takes the last segment with either separator", () => {
    expect(basename("C:\\Users\\user\\veal\\")).toBe("veal");
    expect(basename("/home/u/veal")).toBe("veal");
  });
  it("collapses the Windows profile to ~", () => {
    expect(tilde("C:\\Users\\user\\veal")).toBe("~\\veal");
    expect(tilde("D:\\work")).toBe("D:\\work");
  });
});

describe("deriveProject", () => {
  it("blank root -> ~\\name, to be created", () => {
    expect(deriveProject("scratch", "", HOME)).toEqual({
      name: "scratch",
      path: "C:\\Users\\user\\scratch",
      created: true,
    });
  });
  it("blank name -> basename(root)", () => {
    expect(deriveProject("", "C:\\Users\\user\\Hungy games\\", HOME)).toEqual({
      name: "Hungy games",
      path: "C:\\Users\\user\\Hungy games",
      created: false,
    });
  });
  it("both blank -> nothing to submit", () => {
    expect(deriveProject("  ", " ", HOME)).toBeNull();
  });
  it("joinPath follows the directory's separator style", () => {
    expect(joinPath("/home/u/", "x")).toBe("/home/u/x");
    expect(joinPath("C:\\Users\\user", "x")).toBe("C:\\Users\\user\\x");
  });
});
