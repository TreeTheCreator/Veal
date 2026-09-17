// Pixel geometry for the split tree. Panes are absolutely positioned from a
// flat list (see plan-02-layout.md: nested flex would remount a pane, and
// its PTY, whenever an insert wraps it in a new split).
import type { Dir, LayoutNode, Path } from "./tree";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface DividerRect {
  path: Path;
  index: number;
  dir: Dir;
  rect: Rect;
  /** Pixels the parent split distributes among its children (excluding dividers); the drag scale. */
  span: number;
}

export interface Geometry {
  panes: Map<number, Rect>;
  dividers: DividerRect[];
}

/** Numbers from the mockup: `.layout { padding: 10px }`, `.divider { flex: 0 0 10px }`. */
export const LAYOUT_PADDING = 10;
export const DIVIDER_SIZE = 10;

/** Each neighbour of a divider keeps at least this share of the pair's weight. */
export const MIN_SHARE = 0.15;

export function computeRects(
  root: LayoutNode | null,
  size: { w: number; h: number },
  padding = LAYOUT_PADDING,
  divider = DIVIDER_SIZE,
): Geometry {
  const geo: Geometry = { panes: new Map(), dividers: [] };
  if (!root) return geo;
  const inner: Rect = {
    x: padding,
    y: padding,
    w: Math.max(0, size.w - 2 * padding),
    h: Math.max(0, size.h - 2 * padding),
  };
  walk(root, inner, [], divider, geo);
  return geo;
}

function walk(node: LayoutNode, rect: Rect, path: Path, divider: number, geo: Geometry) {
  if (node.type === "pane") {
    geo.panes.set(node.id, rect);
    return;
  }
  const horiz = node.dir === "row";
  const n = node.children.length;
  const total = node.weights.reduce((a, b) => a + b, 0);
  const span = horiz ? rect.w : rect.h;
  const available = Math.max(0, span - divider * (n - 1));
  let cursor = horiz ? rect.x : rect.y;
  node.children.forEach((child, i) => {
    const len = total > 0 ? (available * node.weights[i]) / total : available / n;
    const childRect: Rect = horiz
      ? { x: cursor, y: rect.y, w: len, h: rect.h }
      : { x: rect.x, y: cursor, w: rect.w, h: len };
    walk(child, childRect, [...path, i], divider, geo);
    cursor += len;
    if (i < n - 1) {
      geo.dividers.push({
        path,
        index: i,
        dir: node.dir,
        span: available,
        rect: horiz
          ? { x: cursor, y: rect.y, w: divider, h: rect.h }
          : { x: rect.x, y: cursor, w: rect.w, h: divider },
      });
      cursor += divider;
    }
  });
}

/**
 * New weight for the left/top neighbour of a divider after the pointer moved
 * `deltaPx` along the split axis. `spanPx` is the split's total length on that
 * axis (the same scale the mockup uses: delta / size * total).
 */
export function dragWeight(
  weights: number[],
  index: number,
  deltaPx: number,
  spanPx: number,
  minShare = MIN_SHARE,
): number {
  const total = weights.reduce((a, b) => a + b, 0);
  const pair = weights[index] + weights[index + 1];
  const delta = spanPx > 0 ? (deltaPx / spanPx) * total : 0;
  const a = weights[index] + delta;
  return Math.max(pair * minShare, Math.min(pair * (1 - minShare), a));
}
