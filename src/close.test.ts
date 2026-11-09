import { describe, expect, it } from "vitest";
import { closeDecision } from "./close";

describe("closing the window (plan-19 6.5)", () => {
  it("asks about unsaved files whatever the setting", () => {
    for (const s of ["ask", "keep", "stop"] as const) {
      expect(closeDecision(1, 0, s), s).toBe("ask");
      expect(closeDecision(2, 1, s), s).toBe("ask");
    }
  });

  it("ends idle shells with no agents running", () => {
    for (const s of ["ask", "keep", "stop"] as const) expect(closeDecision(0, 0, s), s).toBe("stop");
  });

  it("follows the setting with agents running", () => {
    expect(closeDecision(0, 1, "ask")).toBe("ask");
    expect(closeDecision(0, 2, "keep")).toBe("keep");
    expect(closeDecision(0, 1, "stop")).toBe("stop");
  });
});
