import { beforeEach, describe, expect, it, vi } from "vitest";

const settingsGetAll = vi.fn<() => Promise<Record<string, string>>>();
const settingsSet = vi.fn<(k: string, v: string) => Promise<void>>(async () => {});
vi.mock("../db", () => ({ db: { settingsGetAll: () => settingsGetAll(), settingsSet: (k: string, v: string) => settingsSet(k, v) } }));

const { useSettings, isTheme, parsePort, DEFAULT_BYPASS } = await import("./store");

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

  it("accepts only a valid port", () => {
    expect(parsePort("15236")).toBe(15236);
    expect(parsePort(" 80 ")).toBe(80);
    for (const bad of ["", "0", "65536", "99999", "12a", "-1"]) expect(parsePort(bad), bad).toBeNull();
  });
});

describe("proxy mode and its migration (plan-18)", () => {
  const load = async (saved: Record<string, string>) => {
    settingsGetAll.mockResolvedValueOnce(saved);
    await useSettings.getState().load();
    return useSettings.getState();
  };

  it("a new install follows the system with the default bypass, and saves the mode once", async () => {
    const s = await load({});
    expect(s.proxyMode).toBe("system");
    expect(s.proxyBypass).toBe(DEFAULT_BYPASS);
    expect(settingsSet.mock.calls).toEqual([["proxy_mode", "system"]]);
  });

  it("a saved valid port with no mode migrates to manual", async () => {
    const s = await load({ proxy_port: "15236" });
    expect(s.proxyMode).toBe("manual");
    expect(s.proxyPort).toBe("15236");
    expect(settingsSet.mock.calls).toEqual([["proxy_mode", "manual"]]);
  });

  it("a saved blank or invalid port with no mode migrates to system", async () => {
    for (const port of ["", "0", "12a", "99999"]) {
      settingsSet.mockClear();
      expect((await load({ proxy_port: port })).proxyMode, port).toBe("system");
      expect(settingsSet.mock.calls, port).toEqual([["proxy_mode", "system"]]);
    }
  });

  it("an explicit mode wins over the port and is not rewritten", async () => {
    for (const mode of ["off", "manual", "system"] as const) {
      settingsSet.mockClear();
      expect((await load({ proxy_mode: mode, proxy_port: "15236" })).proxyMode).toBe(mode);
      expect(settingsSet).not.toHaveBeenCalled();
    }
  });

  it("an unknown saved mode is treated as missing", async () => {
    expect((await load({ proxy_mode: "auto", proxy_port: "8080" })).proxyMode).toBe("manual");
    expect(settingsSet).toHaveBeenCalledWith("proxy_mode", "manual");
  });

  it("clearing the port after migration keeps manual", async () => {
    await load({ proxy_port: "15236" });
    useSettings.getState().setProxyPort("");
    // What the table now holds: the migrated mode plus the cleared port.
    const s = await load({ proxy_mode: "manual", proxy_port: "" });
    expect(s.proxyMode).toBe("manual");
    expect(s.proxyPort).toBe("");
  });

  it("mode and bypass write through; a cleared bypass stays cleared", async () => {
    useSettings.getState().setProxyMode("off");
    expect(useSettings.getState().proxyMode).toBe("off");
    expect(settingsSet).toHaveBeenCalledWith("proxy_mode", "off");
    useSettings.getState().setProxyBypass(".corp");
    expect(settingsSet).toHaveBeenCalledWith("proxy_bypass", ".corp");
    expect((await load({ proxy_mode: "system", proxy_bypass: "" })).proxyBypass).toBe("");
    expect((await load({ proxy_mode: "system", proxy_bypass: ".corp" })).proxyBypass).toBe(".corp");
  });
});
