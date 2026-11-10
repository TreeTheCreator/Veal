import { describe, expect, it, vi } from "vitest";

const calls: string[] = [];
vi.mock("./components/CloseGuard", () => ({ flushAll: async () => void calls.push("flush") }));
vi.mock("./pty", () => ({
  setCloseMode: async (keep: boolean) => void calls.push(`closeMode:${keep}`),
  shutdownDaemon: async () => (calls.push("shutdown"), true),
}));

const { installUpdate } = await import("./update");

describe("installing an update (plan-19 7.4)", () => {
  it("downloads, saves, stops the background service, then installs", async () => {
    calls.length = 0;
    const labels: string[] = [];
    const update = {
      download: vi.fn(async (cb?: (e: never) => void) => {
        cb?.({ event: "Started", data: { contentLength: 200 } } as never);
        cb?.({ event: "Progress", data: { chunkLength: 100 } } as never);
        calls.push("download");
      }),
      install: vi.fn(async () => void calls.push("install")),
    };
    await installUpdate(update, (l) => labels.push(l));
    expect(calls).toEqual(["download", "flush", "closeMode:false", "shutdown", "install"]);
    expect(labels).toEqual(["Downloading…", "Downloading… 50%", "Installing…"]);
  });

  it("does not stop anything when the download fails", async () => {
    calls.length = 0;
    const update = { download: vi.fn(async () => Promise.reject(new Error("offline"))), install: vi.fn() };
    await expect(installUpdate(update, () => {})).rejects.toThrow("offline");
    expect(calls).toEqual([]);
    expect(update.install).not.toHaveBeenCalled();
  });
});
