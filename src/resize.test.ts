import { describe, expect, it, vi } from "vitest";
import { createResizeScheduler } from "./resize";

describe("createResizeScheduler", () => {
  it("collapses a burst of identical sizes into one call", () => {
    vi.useFakeTimers();
    const apply = vi.fn();
    const s = createResizeScheduler(apply, 50);
    for (let i = 0; i < 10; i++) {
      s.schedule({ cols: 120, rows: 30 });
      vi.advanceTimersByTime(4);
    }
    expect(apply).not.toHaveBeenCalled();
    vi.advanceTimersByTime(50);
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith({ cols: 120, rows: 30 });
    vi.useRealTimers();
  });

  it("calls again only when the size changes", () => {
    vi.useFakeTimers();
    const apply = vi.fn();
    const s = createResizeScheduler(apply, 50);
    s.schedule({ cols: 120, rows: 30 });
    vi.advanceTimersByTime(50);
    s.schedule({ cols: 120, rows: 30 });
    vi.advanceTimersByTime(50);
    expect(apply).toHaveBeenCalledTimes(1);
    s.schedule({ cols: 100, rows: 30 });
    vi.advanceTimersByTime(50);
    expect(apply).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenLastCalledWith({ cols: 100, rows: 30 });
    vi.useRealTimers();
  });

  it("only the last size in a burst is applied", () => {
    vi.useFakeTimers();
    const apply = vi.fn();
    const s = createResizeScheduler(apply, 50);
    s.schedule({ cols: 80, rows: 24 });
    s.schedule({ cols: 90, rows: 24 });
    s.schedule({ cols: 100, rows: 24 });
    vi.advanceTimersByTime(50);
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply).toHaveBeenCalledWith({ cols: 100, rows: 24 });
    vi.useRealTimers();
  });

  it("cancel drops the pending call", () => {
    vi.useFakeTimers();
    const apply = vi.fn();
    const s = createResizeScheduler(apply, 50);
    s.schedule({ cols: 80, rows: 24 });
    s.cancel();
    vi.advanceTimersByTime(100);
    expect(apply).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
