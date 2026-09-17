import { describe, expect, it } from "vitest";
import { stemLength, validateName } from "./names";

describe("validateName (mirrors fs.rs validate_name)", () => {
  it("accepts ordinary names", () => {
    for (const ok of ["a.txt", ".gitignore", "émoji 🐟.md", "console.log", "connect.rs", "nulls.md", "x".repeat(255)]) {
      expect(validateName(ok), ok).toBeNull();
    }
  });

  it("rejects what Windows rejects, with the same messages as Rust", () => {
    const bad: Array<[string, string]> = [
      ["", "name is empty"],
      [" ", "name is empty"],
      [" a", "no leading or trailing spaces"],
      ["a ", "no leading or trailing spaces"],
      [".", "not a valid name"],
      ["..", "not a valid name"],
      ["a/b", "no slashes in a name"],
      ["a\\b", "no slashes in a name"],
      ["a:b", ": is not allowed in a name"],
      ["a?b", "? is not allowed in a name"],
      ["a\tb", "no control characters"],
      ["con", "CON is a reserved name on Windows"],
      ["CON.txt", "CON is a reserved name on Windows"],
      ["lpt1", "LPT1 is a reserved name on Windows"],
      ["a.", "a name cannot end with a dot"],
      ["x".repeat(256), "name is too long"],
    ];
    for (const [name, err] of bad) expect(validateName(name), JSON.stringify(name)).toBe(err);
  });
});

describe("stemLength", () => {
  it("selects the stem, or everything for dotfiles and extensionless names", () => {
    expect(stemLength("App.tsx")).toBe(3);
    expect(stemLength("archive.tar.gz")).toBe(11);
    expect(stemLength(".env")).toBe(4);
    expect(stemLength("Makefile")).toBe(8);
  });
});
