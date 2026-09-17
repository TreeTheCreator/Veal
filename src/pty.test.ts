import { beforeEach, describe, expect, it, vi } from "vitest";

// The Tauri bridge, reduced to what pty.ts touches: `invoke` answers
// pty_spawn with a fresh id and pty_read with an empty buffer, `listen`
// hands back the event callbacks so a test can fire `pty-exit` and
// `pty-data` whenever it likes.
const invoke = vi.fn<(cmd: string, args?: unknown) => Promise<unknown>>();
const events = new Map<string, (ev: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/core", () => ({
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

const empty = () => new ArrayBuffer(0);
const bytes = (s: string) => new TextEncoder().encode(s).buffer as ArrayBuffer;

let nextId = 1;
beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation(async (cmd) =>
    cmd === "pty_spawn" ? nextId++ : cmd === "pty_read" ? empty() : undefined,
  );
});

const opts = { cwd: "C:\\veal", cols: 80, rows: 24 };
const spawn = () => spawnPty(opts, (_b, done) => done());
const reads = () => invoke.mock.calls.filter(([cmd]) => cmd === "pty_read").length;

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

describe("pty output drain (plan-15)", () => {
  /** A `pty_read` that answers `queue` in order, then empties. */
  const readQueue = (queue: ArrayBuffer[]) =>
    invoke.mockImplementation(async (cmd) =>
      cmd === "pty_spawn" ? nextId++ : cmd === "pty_read" ? (queue.shift() ?? empty()) : undefined,
    );

  it("drains once on spawn, so output before the reply is not lost", async () => {
    readQueue([bytes("early"), bytes("more")]);
    const got: string[] = [];
    await spawnPty(opts, (b, done) => {
      got.push(new TextDecoder().decode(b));
      done();
    });
    await tick();
    // Reads until empty: early, more, then the empty that ends the loop.
    expect(got).toEqual(["early", "more"]);
    expect(reads()).toBe(3);
  });

  it("a signal starts a loop; a signal during one asks for one more read, not a second loop", async () => {
    const s = await spawn();
    await tick();
    expect(reads()).toBe(1); // the spawn-time drain, already empty

    // Hold the read open, signal twice meanwhile.
    let answer!: (b: ArrayBuffer) => void;
    invoke.mockImplementation((cmd) =>
      cmd === "pty_read" ? new Promise<ArrayBuffer>((r) => (answer = r)) : Promise.resolve(undefined),
    );
    fire("pty-data", { id: s.id });
    expect(reads()).toBe(2);
    fire("pty-data", { id: s.id });
    fire("pty-data", { id: s.id });
    expect(reads()).toBe(2); // folded into `again`
    answer(empty()); // empty, but `again` was set: one more read
    await tick();
    expect(reads()).toBe(3);
    answer(empty()); // empty and quiet: the loop ends
    await tick();
    expect(reads()).toBe(3);
    // The next signal starts a fresh loop.
    fire("pty-data", { id: s.id });
    expect(reads()).toBe(4);
  });

  it("waits for xterm before the next read", async () => {
    readQueue([bytes("a"), bytes("b")]);
    let release!: () => void;
    const got: string[] = [];
    await spawnPty(opts, (b, done) => {
      got.push(new TextDecoder().decode(b));
      release = done;
    });
    await tick();
    expect(got).toEqual(["a"]);
    expect(reads()).toBe(1); // no second read until xterm is done with "a"
    release();
    await tick();
    expect(got).toEqual(["a", "b"]);
    release();
    await tick();
    expect(reads()).toBe(3);
  });

  it("signals for another id do nothing, and kill stops the loop", async () => {
    const s = await spawn();
    await tick();
    fire("pty-data", { id: s.id + 100 });
    expect(reads()).toBe(1);
    await s.kill();
    fire("pty-data", { id: s.id });
    expect(reads()).toBe(1);
  });
});
