// Context-menu placement shared by the Sidebar and the Files tree (plan-11):
// a right-click near the bottom or right edge would otherwise push items off
// screen. Pure; vitest-tested.

const WIDTH = 170;
const ROW = 32;
const PADDING = 10;

/** `left`/`top` for a menu of `items` rows opened at (x, y), kept inside `w`×`h`. */
export function clampMenu(
  x: number,
  y: number,
  items: number,
  w: number = window.innerWidth,
  h: number = window.innerHeight,
): { left: number; top: number } {
  return {
    left: Math.max(0, Math.min(x, w - WIDTH)),
    top: Math.max(0, Math.min(y, h - (items * ROW + PADDING))),
  };
}
