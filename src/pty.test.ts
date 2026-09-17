import { beforeEach, describe, expect, it, vi } from "vitest";

// The Tauri bridge, reduced to what pty.ts touches: `invoke` answers
// pty_spawn with a fresh id, `listen` hands back the event callbacks so a
// test can fire `pty-exit` whenever it likes.
const invoke = vi.fn<(cmd: string, args?: unknown) => Promise<unknown>>();
const events = new Map<string, (ev: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: unknown;
  },
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, cb: (ev: { payload: unknown }) => void) => {
    events.set(name, cb);
    return () => events.delete(name);
  },
}));

const { spawnPty } = await import("./pty");

const fire = (name: string, payload: unknown) => events.get(name)!({ payload });
const tick = () => new Promise((r) => setTimeout(r, 0));

let nextId = 1;
beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (cmd) => (cmd === "pty_spawn" ? nextId++ : undefined));
});

const spawn = () => spawnPty({ cwd: "C:\veal", cols: 80, rows: 24 }, () => {});

describe("pty exit fan-out (plan-13)", () => {
  it("delivers a normal exit to the handler once", async () => {
    const s = await spawn();
    await tick(); // the listeners are registered asynchronously
    const got: Array<number | null> = [];
    s.onExit((c) => got.push(c));
    fire("pty-exit", { id: s.id, code: 0 });
    expect(got).toEqual([0]);
    fire("pty-exit", { id: s.id, code: 0 });
    expect(got).toEqual([0]);
  });

  it("keeps an exit that arrives before the handler is registered", async () => {
    // Spawn a first session so the listeners exist, then let the *next*
    // id exit before its spawnPty resolves.
    await spawn();
    await tick();
    const early = nextId;
    let resolveSpawn!: (id: number) => void;
    invoke.mockImplementation((cmd) =>
      cmd === "pty_spawn" ? new Promise<number>((r) => (resolveSpawn = r)) : Promise.resolve(undefined),
    );
    const pending = spawn();
    fire("pty-exit", { id: early, code: 3 });
    resolveSpawn(early);
    const s = await pending;
    const got: Array<number | null> = [];
    s.onExit((c) => got.push(c));
    expect(got).toEqual([]); // on a microtask, like a real event would be
    await tick();
    expect(got).toEqual([3]);
    // Consumed: a second registration does not see it again.
    const again: unknown[] = [];
    s.onExit((c) => again.push(c));
    await tick();
    expect(again).toEqual([]);
  });

  it("does not leak an early exit to another id, and unsubscribing cancels it", async () => {
    await spawn();
    await tick();
    fire("pty-exit", { id: 42, code: 1 });
    const s = await spawn();
    expect(s.id).not.toBe(42);
    const got: unknown[] = [];
    s.onExit((c) => got.push(c));
    await tick();
    expect(got).toEqual([]);

    fire("pty-exit", { id: s.id + 1, code: 9 });
    invoke.mockImplementation(async (cmd) => (cmd === "pty_spawn" ? s.id + 1 : undefined));
    const t = await spawn();
    const off = t.onExit((c) => got.push(c));
    off();
    await tick();
    expect(got).toEqual([]);
  });
});
