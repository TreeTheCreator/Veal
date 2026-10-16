// Agent activity in the Files tree: while an agent is running in a project
// (plan-08's detection, the same rule as the sidebar dot), the paths in that
// project's watcher batches are recorded as "touched", and the tree lights
// each touched row and every folder above it. The agent's latest path stays
// lit until it moves on or exits — Files is its own tab, so a flash would be
// missed — and earlier ones fade after a few seconds — so a
// collapsed `src` shows the agent is working somewhere inside. The watcher
// cannot say who wrote a file, so Veal's own writes (save, create, rename,
// delete) are marked just before they happen and skipped, and build output
// and `.veal/` are never counted. Runtime only, never persisted. The
// functions are pure and vitest-tested; the store owns the clock.
import { create } from "zustand";
import { parentPath } from "./watch";
import { pathKey } from "./tree";

/** How long a touched path stays lit once it is no longer the latest. */
export const TOUCH_MS = 8000;
/** How long a write of our own is ignored when its watcher event arrives. */
export const OWN_MS = 3000;

/** Path segments whose churn is tooling, not the agent (`.git` never reaches us). */
const IGNORED = new Set([".veal", "node_modules", "dist", "build", "target", ".next", ".turbo", ".cache", "__pycache__"]);

/** pathKey → expiry (ms); `Infinity` marks the agent's latest path. */
export type Touches = Map<string, number>;

export interface Lit {
  /** Touched rows themselves. */
  paths: Set<string>;
  /** Folders with a touched path somewhere beneath them. */
  dirs: Set<string>;
}

const segments = (key: string) => key.split("\\").filter(Boolean);

/** True when `path` sits under `root` and no segment between them is ignored. */
export function counts(root: string, path: string): boolean {
  const r = pathKey(root).replace(/\\+$/, "");
  const k = pathKey(path);
  if (!k.startsWith(r + "\\")) return false;
  return !segments(k.slice(r.length)).some((s) => IGNORED.has(s));
}

/** Drops entries that expired at `now`; returns the same map when nothing did. */
export function prune(m: Touches, now: number): Touches {
  let next: Touches | null = null;
  for (const [k, until] of m) {
    if (until > now) continue;
    next ??= new Map(m);
    next.delete(k);
  }
  return next ?? m;
}

/** The latest path starts fading: every `Infinity` expiry becomes `now + TOUCH_MS`. */
export function settle(m: Touches, now: number): Touches {
  let next: Touches | null = null;
  for (const [k, until] of m) {
    if (until !== Infinity) continue;
    next ??= new Map(m);
    next.set(k, now + TOUCH_MS);
  }
  return next ?? m;
}

/**
 * Records `paths`, skipping our own writes and ignored paths: the last one
 * counted becomes the latest (lit until `settle`), the rest fade at
 * `now + TOUCH_MS`, and the previous latest starts fading. A batch with
 * nothing counted leaves the latest where it was.
 */
export function record(m: Touches, root: string, paths: string[], own: Touches, now: number): Touches {
  const counted = paths.filter((p) => (own.get(pathKey(p)) ?? 0) <= now && counts(root, p)).map(pathKey);
  if (counted.length === 0) return prune(m, now);
  const next = new Map(settle(prune(m, now), now));
  for (const key of counted) next.set(key, now + TOUCH_MS);
  next.set(counted[counted.length - 1], Infinity);
  return next;
}

/** The rows to light: each live touched path, and every folder between it and the root. */
export function lit(m: Touches | undefined, root: string, now: number): Lit {
  const paths = new Set<string>();
  const dirs = new Set<string>();
  const rootKey = pathKey(root).replace(/\\+$/, "");
  for (const [key, until] of m ?? []) {
    if (until <= now) continue;
    paths.add(key);
    for (let dir = parentPath(key); dir && dir.length > rootKey.length; dir = parentPath(dir)) {
      if (dirs.has(dir)) break;
      dirs.add(dir);
    }
  }
  return { paths, dirs };
}

/** The earliest finite expiry in `m`, for the next prune. */
export function nextExpiry(m: Iterable<Touches>): number | null {
  let soonest: number | null = null;
  for (const t of m) {
    for (const until of t.values()) if (until !== Infinity && (soonest === null || until < soonest)) soonest = until;
  }
  return soonest;
}

interface ActivityState {
  byProject: Map<number, Touches>;
  /** Our own writes in flight (pathKey → ignore until). */
  own: Touches;
  /** A watcher batch for a project with a live agent. */
  touch(projectId: number, root: string, paths: string[]): void;
  /** Called just before Veal itself writes `paths`. */
  markOwn(...paths: string[]): void;
  /** The project's agent exited: its latest path starts fading. */
  settle(projectId: number): void;
}

let timer: ReturnType<typeof setTimeout> | null = null;

export const useActivity = create<ActivityState>((set, get) => {
  /** Prunes everything expired and re-arms one timer for the next expiry. */
  const sweep = () => {
    timer = null;
    const now = Date.now();
    const byProject = new Map<number, Touches>();
    let changed = false;
    for (const [id, t] of get().byProject) {
      const kept = prune(t, now);
      if (kept !== t) changed = true;
      if (kept.size > 0) byProject.set(id, kept);
      else changed = true;
    }
    if (changed) set({ byProject });
    const soonest = nextExpiry(byProject.values());
    if (soonest !== null) timer = setTimeout(sweep, Math.max(0, soonest - now) + 50);
  };
  const arm = () => {
    if (timer === null) timer = setTimeout(sweep, TOUCH_MS + 50);
  };

  return {
    byProject: new Map(),
    own: new Map(),

    touch(projectId, root, paths) {
      const now = Date.now();
      const prev = get().byProject.get(projectId) ?? new Map();
      const next = record(prev, root, paths, get().own, now);
      if (next.size === 0 && prev.size === 0) return;
      const byProject = new Map(get().byProject);
      byProject.set(projectId, next);
      set({ byProject });
      arm();
    },

    markOwn(...paths) {
      const now = Date.now();
      const own = new Map(prune(get().own, now));
      for (const p of paths) own.set(pathKey(p), now + OWN_MS);
      set({ own });
    },

    settle(projectId) {
      const prev = get().byProject.get(projectId);
      if (!prev) return;
      const next = settle(prev, Date.now());
      if (next === prev) return;
      const byProject = new Map(get().byProject);
      byProject.set(projectId, next);
      set({ byProject });
      arm();
    },
  };
});
