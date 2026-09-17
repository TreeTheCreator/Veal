// Debounce + dedupe for pty_resize: a ResizeObserver fires on every layout
// tick while a divider is dragged, but the PTY only needs the final size, and
// only when it actually changed.

export interface Size {
  cols: number;
  rows: number;
}

export interface ResizeScheduler {
  schedule(size: Size): void;
  /** Cancel any pending call. */
  cancel(): void;
}

export const RESIZE_DEBOUNCE_MS = 50;

export function createResizeScheduler(
  apply: (size: Size) => void,
  delayMs = RESIZE_DEBOUNCE_MS,
  timers: Pick<typeof globalThis, "setTimeout" | "clearTimeout"> = globalThis,
): ResizeScheduler {
  let last: Size | null = null;
  let pending: Size | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  return {
    schedule(size) {
      pending = size;
      if (timer !== null) timers.clearTimeout(timer);
      timer = timers.setTimeout(() => {
        timer = null;
        const next = pending;
        pending = null;
        if (!next) return;
        if (last && last.cols === next.cols && last.rows === next.rows) return;
        last = next;
        apply(next);
      }, delayMs);
    },
    cancel() {
      if (timer !== null) timers.clearTimeout(timer);
      timer = null;
      pending = null;
    },
  };
}
