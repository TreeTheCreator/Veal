import { beforeEach, describe, expect, it, vi } from "vitest";

const settingsGetAll = vi.fn<() => Promise<Record<string, string>>>();
const settingsSet = vi.fn<(k: string, v: string) => Promise<void>>(async () => {});
vi.mock("../db", () => ({ db: { settingsGetAll: () => settingsGetAll(), settingsSet: (k: string, v: string) => settingsSet(k, v) } }));

const { useSettings, isTheme, proxyForSpawn } = await import("./store");

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

describe("proxy port setting", () => {
  it("loads, defaults to none, and writes through", async () => {
    settingsGetAll.mockResolvedValueOnce({});
    await useSettings.getState().load();
    expect(useSettings.getState().proxyPort).toBe("");
    settingsGetAll.mockResolvedValueOnce({ proxy_port: "15236" });
    await useSettings.getState().load();
    expect(useSettings.getState().proxyPort).toBe("15236");
    useSettings.getState().setProxyPort("8080");
    expect(settingsSet).toHaveBeenCalledWith("proxy_port", "8080");
  });

  it("builds the proxy URL only from a valid port", () => {
    expect(proxyForSpawn("15236")).toBe("http://127.0.0.1:15236");
    expect(proxyForSpawn(" 80 ")).toBe("http://127.0.0.1:80");
    for (const bad of ["", "0", "65536", "99999", "12a", "-1"]) expect(proxyForSpawn(bad), bad).toBeUndefined();
  });
});
