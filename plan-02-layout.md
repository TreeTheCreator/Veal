# Veal — plan for step 3: flexible pane layout + pane chrome

Status: APPROVED 2026-09-17 (manual pane type; cap 4; exit 0 auto-closes; 15 % drag clamp).

Scope: the Terminal view goes from one hard-coded pane to N panes in a
resizable split tree, each with the header/badge/accent chrome from the
mockup. Still no sidebar/projects (step 4), no persistence (step 4), no
tab switching. `cwd` stays hard-coded to `C:\Users\user\veal`; the pane
header's project name is `basename(cwd)` until the sidebar supplies it.

No Rust changes. No new dependencies (zustand is already installed).

## Layout model (`src/layout/tree.ts`, pure, vitest-tested)

```ts
type Node = Split | Pane;
type Split = { type: "split"; dir: "row" | "col"; weights: number[]; children: Node[] };
type Pane  = { type: "pane"; id: number; kind: "shell" | "agent"; cwd: string };
```

Same shape as the mockup. Ops, all pure (return a new tree):

| Op                             | Behaviour (from the mockup)                                                                                                                                                                        |
|--------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `insert(tree, p)`              | empty → `p`; single pane → row `[old, p]` weights `[1,1]`; root row with 1 child → push; else split the root child with the fewest panes into a `col` `[child, p]` `[1,1]` (or push if it is already a col split) |
| `remove(tree, id)`             | drop the leaf and its weight; a split left with one child collapses to that child; empty root → `null`                                                                                             |
| `setKind(tree, id, kind)`      | flips the badge                                                                                                                                                                                    |
| `setWeights(tree, path, i, a)` | divider `i` of the split at `path` gets weights `[a, pair - a]`                                                                                                                                    |
| `panes(tree)`                  | leaves in DOM order                                                                                                                                                                                |

Insert order for 4 panes is therefore: `1` → `1 | 2` → `(1/3) | 2` → `(1/3) | (2/4)`.

**Soft cap: 4** (mockup's `CAP`, low end of the spec's 4–6). At the 900×600
minimum window a 2×2 grid gives each pane ~430×240 px ≈ 55 cols × 13 rows,
which is the smallest a `claude` TUI is still usable at. One constant,
`PANE_CAP`, so it is a one-line change. New-terminal button is disabled at
the cap with the hint "Soft cap of 4 reached".

## Geometry (`src/layout/rects.ts`, pure, vitest-tested)

Instead of nesting flex containers, `rects(tree, {w, h})` walks the tree and
returns `{ panes: Map<id, Rect>, dividers: Array<{path, i, dir, rect}> }` in
pixels. The layout area is `position: relative`; every pane and divider is
`position: absolute` at its rect.

Why: React remounts a component when it moves to a different parent, even with
a stable `key`. Under nested flex, inserting pane 3 wraps pane 1 in a new col
split, which would remount `<TerminalPane 1>` and kill its PTY. With computed
rects the pane components are a flat list keyed by id and never move in the
React tree; only their `style` changes. The existing `ResizeObserver` in
`TerminalPane` already handles the fit + debounced `pty_resize` when the rect
changes.

Numbers (mockup): layout padding **10 px**, divider thickness **10 px** (the
visible 2 px line is centred in it), pane border-radius **10 px**, left accent
stripe **3 px**. Weight → pixel: `(available - 10 × (n-1)) × w_i / Σw`.

## Divider drag

Pointer capture on the divider (`pointerdown` → `setPointerCapture`), same as
the mockup. Delta in px is converted to weight in the parent split's units;
each neighbour is clamped to keep **≥ 15 % of the pair's combined weight**
(mockup's `pair * 0.15 … 0.85`). Weights write straight into the store on every
`pointermove`; the PTY resize is already debounced 50 ms downstream so this is
not chatty. Divider hover / dragging colour: `--accent`.

## Store (`src/layout/store.ts`, zustand)

```ts
{ tree: Node | null; focused: number | null; nextId: number;
  addPane(kind, cwd); closePane(id); flipKind(id); focus(id); setWeights(path, i, a) }
```

Focused pane gets `.focused` (border `--line-strong`). Clicking anywhere in a
pane focuses it and calls `term.focus()`. New pane → focused. Closing the
focused pane → focus moves to the first remaining leaf.

On app start the store is seeded with **one shell pane** in `CWD`, so the
window still opens straight into a prompt like step 2.

## Pane chrome (`TerminalPane` grows a header)

Header **36 px** tall, `--panel` background, 1 px `--line` bottom border,
contents left→right, exactly the mockup's:

1. badge — `shell` (blue `--shell` on `--shell-soft`) or `agent` (red
   `--agent` on `--agent-soft`), 10 px uppercase, letter-spacing 0.1 em,
   pill; **clickable, flips the kind** (title "Flip pane type")
2. project name — mono 12.5 px, `--text`
3. cwd — mono 12 px, `--muted`, ellipsis, takes the remaining width
4. close `×` — 26×26 px, hover `--panel-2`; kills the PTY via the existing unmount path

Pane box: `--bg-sunk`, 1 px `--line` border, 3 px left border in the kind
colour, radius 10 px, `overflow: hidden`.

## Toolbar

`+ New terminal` primary button (mockup `.btn.primary`) opens the mockup's
type popover (Shell / Agent, with the kind swatch and a one-line description);
picking one calls `addPane(kind, CWD)`. Hint next to it: "Opens in ~\veal" or
the cap message. Top-bar right shows `n/4 panes` in mono, replacing the
`step 2` pill. Empty layout (all panes closed) shows the mockup's
"No panes open." block.

## Process exit

- exit code **0** → the pane closes itself (tmux/Zellij behaviour; typing
  `exit` is how you close a pane from the keyboard).
- non-zero / unknown → the pane stays with the step-2 footer
  "process exited (code N)" so the failure is visible; close it with `×`.

## Decisions to confirm

1. **Pane type is manual** for now: chosen in the popover, flippable via the
   badge. Auto-detecting `claude` from the running command needs process-title
   or child-process inspection on the Rust side and is deferred; the spec
   leaves this open. Agent panes still spawn the plain shell — they are a
   label and accent only, not an auto-`claude` launcher.
2. **Cap = 4** (vs the spec's "4–6").
3. **Exit code 0 auto-closes** the pane; non-zero keeps it.
4. **Drag clamp 15 % of the pair** (mockup) rather than a pixel minimum.

## Tests (vitest)

- `tree.test.ts`: the 4-insert sequence above produces the expected shape;
  removing pane 3 from `(1/3) | (2/4)` collapses back to `1 | (2/4)` keeping
  the row weights; removing the last pane yields `null`; insert past the cap
  is rejected by the store.
- `rects.test.ts`: row `[1,1]` in 1000×600 with padding 10 and divider 10 →
  panes `x=10,w=485` and `x=505,w=485`, divider at `x=495,w=10`; row `[3,1]`
  splits 727.5 / 242.5; nested col inside row uses the row child's rect.
- Drag helper: a +100 px move on a 1000 px row `[1,1]` yields `[1.2, 0.8]`;
  a −900 px move clamps to `[0.3, 1.7]`.

## Acceptance (manual, dev window)

1. Window opens with one shell pane, blue stripe, `shell` badge, `veal`, cwd, `×`.
2. `+ New terminal` → Agent → second pane opens to the right, red stripe, focused, prompt in `C:\Users\user\veal`.
3. Third and fourth panes open as `(1/3) | (2/4)`; the button disables at 4/4.
4. Drag the vertical divider: both PTYs report the new `$Host.UI.RawUI.WindowSize`; no pane remounts (the shell history in each survives).
5. Click a badge → it flips agent ⇄ shell; stripe colour follows.
6. `exit` in pane 3 → it disappears, layout collapses to `1 | (2/4)`, the other three shells keep their scrollback.
7. `×` on a pane → its `pwsh.exe` is gone from `Get-Process`.
8. Close the window → no leftover `pwsh.exe`.
9. Commit: `Add split-tree pane layout with draggable dividers and pane chrome`.
