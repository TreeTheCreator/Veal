import { describe, expect, it } from "vitest";
import { effectiveMode } from "./theme";

describe("effectiveMode", () => {
  it("system follows the OS, the others pin", () => {
    expect(effectiveMode("system", true)).toBe("light");
    expect(effectiveMode("system", false)).toBe("dark");
    expect(effectiveMode("dark", true)).toBe("dark");
    expect(effectiveMode("light", false)).toBe("light");
  });
});
