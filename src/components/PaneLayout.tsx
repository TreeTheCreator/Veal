// Renders the split tree as absolutely positioned panes and dividers inside
// one measured container. Panes are a flat list keyed by id so they keep
// their PTY across tree reshapes; only their `style` changes.
import { useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { computeRects, dragWeight, type DividerRect, type Rect } from "../layout/rects";
import { useLayout } from "../layout/store";
import { panes, splitAt } from "../layout/tree";
import { TerminalPane } from "./TerminalPane";

function toStyle(r: Rect): CSSProperties {
  return { left: r.x, top: r.y, width: r.w, height: r.h };
}

export function PaneLayout({ emptyHint }: { emptyHint: string }) {
  const tree = useLayout((s) => s.tree);
  const focused = useLayout((s) => s.focused);
  const agents = useLayout((s) => s.agents);
  const { closePane, focus } = useLayout.getState();

  const hostRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize({ w: width, h: height });
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  const geo = useMemo(() => computeRects(tree, size), [tree, size]);
  // Stable render order, independent of where a pane sits in the tree.
  const leaves = useMemo(() => panes(tree).sort((a, b) => a.id - b.id), [tree]);

  // Closing a pane with a *detected* agent asks first (plan-16); the manual
  // Agent label does not count, matching the sidebar dot. A clean `exit`
  // from the shell still closes at once: it comes through `closePane`.
  const [confirm, setConfirm] = useState<{ id: number; agent: string } | null>(null);
  const requestClose = (id: number) => {
    const agent = useLayout.getState().agents.get(id);
    if (agent) setConfirm({ id, agent });
    else closePane(id);
  };

  useEffect(() => {
    if (!confirm) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setConfirm(null);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [confirm]);

  return (
    <div className="layout" ref={hostRef}>
      {leaves.length === 0 && (
        <div className="empty-layout">
          No panes open.
          <br />
          <span className="hint">{emptyHint}</span>
        </div>
      )}
      {size.w > 0 &&
        leaves.map((p) => (
          <TerminalPane
            key={p.id}
            pane={p}
            agent={agents.get(p.id) ?? null}
            focused={p.id === focused}
            style={toStyle(geo.panes.get(p.id)!)}
            onFocus={() => focus(p.id)}
            onClose={() => requestClose(p.id)}
          />
        ))}
      {geo.dividers.map((d) => (
        <Divider key={`${d.path.join(".")}:${d.index}`} divider={d} />
      ))}
      {confirm && (
        <div className="scrim open" onPointerDown={(e) => e.target === e.currentTarget && setConfirm(null)}>
          <div className="modal" role="dialog" aria-modal="true">
            <h3>Close this pane?</h3>
            <div className="field">
              <span>
                <b>{confirm.agent}</b> is running in it. Closing the pane ends the session.
              </span>
            </div>
            <div className="row">
              <button type="button" className="btn ghost" onClick={() => setConfirm(null)} autoFocus>
                Cancel
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  closePane(confirm.id);
                  setConfirm(null);
                }}
              >
                Close pane
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Divider({ divider }: { divider: DividerRect }) {
  const [dragging, setDragging] = useState(false);
  const { path, index, dir, rect, span } = divider;
  const horiz = dir === "row";

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    setDragging(true);
    const start = horiz ? e.clientX : e.clientY;
    // Snapshot the weights at drag start; every move is relative to them.
    const split = splitAt(useLayout.getState().tree, path);
    if (!split) return;
    const weights = split.weights.slice();
    const setWeights = useLayout.getState().setWeights;

    const move = (ev: PointerEvent) => {
      const delta = (horiz ? ev.clientX : ev.clientY) - start;
      setWeights(path, index, dragWeight(weights, index, delta, span));
    };
    const up = () => {
      setDragging(false);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  return (
    <div
      className={`divider ${dir}${dragging ? " dragging" : ""}`}
      style={toStyle(rect)}
      onPointerDown={onPointerDown}
      role="separator"
      aria-orientation={horiz ? "vertical" : "horizontal"}
    />
  );
}
