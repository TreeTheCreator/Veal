import { beforeEach, describe, expect, it, vi } from "vitest";

const settingsGetAll = vi.fn<() => Promise<Record<string, string>>>();
const settingsSet = vi.fn<(k: string, v: string) => Promise<void>>(async () => {});
vi.mock("../db", () => ({ db: { settingsGetAll: () => settingsGetAll(), settingsSet: (k: string, v: string) => settingsSet(k, v) } }));

const { useSettings, isTheme } = await import("./store");

beforeEach(() => {
  settingsSet.mockClear();
});

describe("theme setting (plan-11)", () => {
  it("parses the three modes and falls back to system", async () => {
    for (const [raw, want] of [["light", "light"], ["dark", "dark"], ["system", "system"], ["neon", "system"], [undefined, "system"]] as const) {
      settingsGetAll.mockResolvedValueOnce(raw === undefined ? {} : { theme: raw });
      await useSettings.getState().load();
      expect(useSettings.getState().theme, String(raw)).toBe(want);
    }
    expect(isTheme("light")).toBe(true);
    expect(isTheme("blue")).toBe(false);
  });

  it("setTheme writes through", () => {
    useSettings.getState().setTheme("light");
    expect(useSettings.getState().theme).toBe("light");
    expect(settingsSet).toHaveBeenCalledWith("theme", "light");
  });
});
