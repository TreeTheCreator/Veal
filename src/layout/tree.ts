// Split-tree layout model. Same shape as the mockup: a split holds children
// with flex weights, a pane is a leaf. Every op is pure and returns a new
// tree; untouched subtrees are shared, so React can diff by identity.

export type Dir = "row" | "col";

export interface PaneNode {
  type: "pane";
  id: number;
  cwd: string;
}

export interface SplitNode {
  type: "split";
  dir: Dir;
  weights: number[];
  children: LayoutNode[];
}

export type LayoutNode = PaneNode | SplitNode;

/** Index path from the root to a split (`[]` is the root itself). */
export type Path = number[];

/** Soft cap on simultaneous panes; the mockup's `CAP`. */
export const PANE_CAP = 4;

/** Leaves in DOM order. */
export function panes(node: LayoutNode | null, out: PaneNode[] = []): PaneNode[] {
  if (!node) return out;
  if (node.type === "pane") out.push(node);
  else node.children.forEach((c) => panes(c, out));
  return out;
}

function split(dir: Dir, children: LayoutNode[], weights = children.map(() => 1)): SplitNode {
  return { type: "split", dir, weights, children };
}

// Where the next pane goes: fill the root row to 2, then split the root child
// with the fewest panes into a column. Four panes end up as (1/3) | (2/4).
// A root that is not a row — `remove` collapsed the row onto its last
// column — is wrapped in one first (plan-13), so the grid comes back rather
// than every later pane stacking into that column.
export function insert(root: LayoutNode | null, p: PaneNode): LayoutNode {
  if (!root) return p;
  if (root.type !== "split" || root.dir !== "row") return split("row", [root, p]);
  if (root.children.length < 2) {
    return { ...root, children: [...root.children, p], weights: [...root.weights, 1] };
  }
  let idx = 0;
  let best = Infinity;
  root.children.forEach((c, i) => {
    const n = panes(c).length;
    if (n < best) {
      best = n;
      idx = i;
    }
  });
  const target = root.children[idx];
  const replaced: LayoutNode =
    target.type === "split"
      ? { ...target, children: [...target.children, p], weights: [...target.weights, 1] }
      : split("col", [target, p]);
  const children = root.children.slice();
  children[idx] = replaced;
  return { ...root, children };
}

// Drop the leaf and its weight; a split left with one child collapses to that
// child so the tree never holds a degenerate split.
export function remove(root: LayoutNode | null, id: number): LayoutNode | null {
  if (!root) return null;
  if (root.type === "pane") return root.id === id ? null : root;
  const children: LayoutNode[] = [];
  const weights: number[] = [];
  let changed = false;
  root.children.forEach((c, i) => {
    const r = remove(c, id);
    if (r !== c) changed = true;
    if (r) {
      children.push(r);
      weights.push(root.weights[i]);
    }
  });
  if (!changed) return root;
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  return { ...root, children, weights };
}

export function splitAt(root: LayoutNode | null, path: Path): SplitNode | null {
  let node = root;
  for (const i of path) {
    if (!node || node.type !== "split") return null;
    node = node.children[i];
  }
  return node && node.type === "split" ? node : null;
}

/** Divider `i` of the split at `path` moves so its neighbours weigh `[a, pair - a]`. */
export function setWeights(root: LayoutNode | null, path: Path, i: number, a: number): LayoutNode | null {
  if (!root || root.type !== "split") return root;
  if (path.length === 0) {
    const weights = root.weights.slice();
    const pair = weights[i] + weights[i + 1];
    weights[i] = a;
    weights[i + 1] = pair - a;
    return { ...root, weights };
  }
  const [head, ...rest] = path;
  const child = root.children[head];
  const next = setWeights(child, rest, i, a);
  if (next === child) return root;
  const children = root.children.slice();
  children[head] = next as LayoutNode;
  return { ...root, children };
}
