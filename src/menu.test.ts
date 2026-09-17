import { describe, expect, it } from "vitest";
import { clampMenu } from "./menu";

describe("clampMenu", () => {
  it("leaves a menu that fits where it was opened", () => {
    expect(clampMenu(100, 100, 4, 800, 600)).toEqual({ left: 100, top: 100 });
  });
  it("pulls a menu near the bottom or right edge back inside the window", () => {
    expect(clampMenu(700, 590, 4, 800, 600)).toEqual({ left: 630, top: 600 - (4 * 32 + 10) });
    expect(clampMenu(790, 10, 1, 800, 600)).toEqual({ left: 630, top: 10 });
  });
  it("never goes negative", () => {
    expect(clampMenu(0, 0, 40, 800, 600)).toEqual({ left: 0, top: 0 });
  });
});
