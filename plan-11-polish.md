# Veal — plan for step 12: reordering, ◂, light theme, loose ends

Status: BUILT 2026-09-17 — written and built in one go at the user's
request. Every decision below takes the option that leaves the app
simplest.

Scope, four riders that close what earlier plans left open:

1. **Tasks: ◂ back-step and ↑/↓ reordering** (the rest of plan-05,
   decision 9). A card in *In progress* or *Done* gets ◂ (the reverse of
   ▸); every card gets ↑ / ↓ that swap it with its neighbour in the same
   column. Order is the file's array order for a project, `position` for
   the Inbox — nothing new is stored.
2. **Light theme** (the row plan-03 dropped). The mockup's light tokens
   and its Appearance seg (System / Dark / Light) are ported verbatim;
   xterm follows the effective mode.
3. **Files tree survives a restart** (plan-04, decision 7). Expanded
   directories and the open file per project are written to the
   `settings` table and restored on the first look.
4. **Sidebar context menu stays on screen** — plan-09's `clampMenu`
   becomes shared.

Not done, on purpose (decision 5): **Save all** in the close guard. The
modal keeps *Discard and quit / Cancel*; `Ctrl+S` first. The live text of
every dirty file lives in stashed `EditorState`s and the open editor, and
saving several files with conflict handling from a modal is more UI than
the case deserves — the same call as plan-06.

## Dependencies

None.

## 1. Tasks: ◂ and ↑/↓

Rust (`tasks.rs`): `backward(status)` is not needed server-side — ◂ is
`set_status` with the previous status (`done → doing → todo`); the
`backward()` helper lives in `board.ts` next to `forward()`.

`tasks::shift(root, id, delta: i32)` swaps the task with the nearest task
**of the same status** before (`-1`) or after (`+1`) it in the file's
array; at the edge it is a no-op that still returns the list. Unknown id
→ `no task "<id>"`. `Db::inbox_shift(id, delta)` swaps the two rows'
`position` values the same way (neighbour = next same-status row by
position). Command `tasks_shift(root, id, delta)` under the lock.

Frontend: `tasks.shift`, store `shift(projectId, root, id, delta)` via
`apply`. `TaskCard` actions become `✎ ↑ ↓ ◂ ▸ ×` — ◂ only when the
status is not `todo`, ▸ becomes ↺ on `done` as today. ↑ on the first card
of a column and ↓ on the last are disabled. The card's actions are on
hover/focus already (mockup `.acts`); no layout change.

## 2. Light theme

- `styles.css`: the mockup's `@media (prefers-color-scheme: light)
  :root:not([data-theme="dark"])` and `:root[data-theme="light"]` blocks,
  verbatim. Everything is on tokens already; the one hard-coded colour
  set is xterm's.
- `settings/store.ts`: `theme: "system" | "dark" | "light"` (key
  `theme`, default `system`), `setTheme`. `App` applies it: `system` →
  no `data-theme` attribute, else `data-theme=<mode>` on `<html>`.
- `SettingsView`: the Appearance row at the top — "Follows the OS unless
  set here" — with the three-way seg.
- **xterm**: `terminalTheme()` reads the effective tokens from
  `getComputedStyle(document.documentElement)` (`--bg-sunk`, `--text`,
  `--accent`, `--sel`) and returns xterm's `ITheme`. `TerminalPane` sets
  `term.options.theme` at mount and whenever the setting or
  `matchMedia("(prefers-color-scheme: light)")` changes — one shared
  `useEffectiveTheme()` hook returns `"dark" | "light"` and re-renders
  on either.
- CodeMirror uses the tokens through `var()` already; `viewerTheme`'s
  `{ dark: true }` only picks CodeMirror's base defaults for things the
  theme does not set, and the theme sets all of them — unchanged.

## 3. Tree state across restarts

- Key `files.<projectId>` in `settings`, value `{"expanded":[…real
  paths…],"open":"…"|null}`; written debounced (250 ms, like the layout)
  from a `useFiles.subscribe` that fires when a project's `expanded` or
  `open` changed. Removed (written as `""`) by `forget`.
- Boot: `App.boot()` hands the raw settings map to
  `useFiles.getState().restore(map)`, which parses every `files.*` key
  into `saved: Map<projectId, Saved>`. `ensure(projectId, root)`, when
  `saved` has the project: seeds `expanded`, lists the root **and every
  expanded directory** (in parallel — `flatten` only descends once the
  parent's entries arrive, so order does not matter), and opens the file.
  A directory or file that no longer exists just errors as today (the
  tree shows the error row, the viewer its message) — no special case.
- The Settings "Restore layout on launch" toggle also governs this: off
  → nothing is restored (the keys are still written).

## 4. Sidebar menu clamp

`clampMenu` moves from `FilesView` to `src/menu.ts`; the Sidebar uses it
with its one item.

## Tests

- Rust: `shift` moves within the same status only, skips other statuses
  in between, no-op at the edges, error on an unknown id, other tasks and
  extras untouched; `inbox_shift` same over `position`.
- vitest `tasks/board.test.ts`: `backward`; `tasks/store.test.ts`:
  `shift` calls the API.
- vitest `files/persist.test.ts`: `serializeFiles` / `parseFiles`
  round-trip, garbage → null, `restore` seeds `saved`; `ensure` with a
  saved state lists the root and each expanded dir and opens the file;
  with "restore layout" off it lists only the root.
- vitest `settings/store.test.ts` (new, small): `theme` parses `system`
  / `dark` / `light`, anything else → `system`.
- vitest `menu.test.ts`: `clampMenu` keeps a menu inside a 800×600
  window.

## Acceptance (manual, dev window)

1. Tasks: ◂ on an *In progress* card → *To do*; ◂ on *Done* → *In
   progress*; ↑/↓ reorder within a column and `tasks.json`'s array order
   changes accordingly, nothing else in the file; ↑ on the top card is
   disabled; Inbox cards reorder too and survive a reload.
2. Appearance → Light: every surface flips (top bar, sidebar, panes,
   tree, viewer, board, modals), the terminal background goes white with
   dark text, the cursor is the light accent; Dark flips back; System
   follows the OS setting live (toggle Windows' app mode).
3. Expand `src` and `src/files`, open `App.tsx`, quit, relaunch → same
   tree, same file. Unpin the project → its key is gone from `veal.db`.
   Turn "Restore layout on launch" off, relaunch → collapsed tree.
4. Right-click the last project in a full sidebar near the bottom edge →
   the menu is fully visible.
5. `pnpm test`, `cargo test`, `pnpm build` green.
6. Commit: `Add task reordering and back-step, light theme, tree restore, menu clamp`.

## Findings during acceptance (2026-09-17)

- Acceptance 1–4 ran in the dev window over CDP: 14 task checks (↑/↓
  change the file's array order and nothing else, edge buttons disabled,
  ◂ steps back through both columns, the Inbox order survives a reload),
  7 theme checks, 4 tree-restore checks and 2 menu checks — all passing.
  Seeded tasks were removed; the `.veal/` dir the run created was
  removed.
- **xterm read the old tokens.** The theme was applied to `<html>` in an
  `App` effect, but React runs children's effects first, so a pane's
  "re-read the palette" effect saw the previous mode. `applyTheme` now
  runs synchronously inside `useSettings.setTheme` / `load`, before any
  render; the `App` effect is gone.
- System mode was checked live by flipping Windows' app mode with a
  `WM_SETTINGCHANGE ImmersiveColorSet` broadcast: the media query, the
  tokens and xterm's background all followed within a second, both ways.
  The OS setting was restored to dark afterwards.
- A real quit-and-relaunch restored `src`, `src\files` and the open
  `tree.ts` from `files.1` in `veal.db`.
- The dev hook `window.veal` now also exposes `useLayout` and
  `useSettings`; a dynamic `import("/src/…")` from the driver can land on
  a different module instance after HMR, the hook cannot.
- Not done, as planned: Save all in the close guard (decision 5).

## Decisions (simplest option each)

1. **Reordering by ↑/↓ buttons**, swapping array positions. Alternative:
   drag and drop (drop targets, auto-scroll, a library) — rejected as the
   opposite of simple.
2. **◂ is `set_status` with the previous status**; no new Rust. Alt: a
   `backward` command.
3. **Theme: the mockup's three-way seg, System default, applied as a
   `data-theme` attribute**; xterm reads the tokens from computed style
   rather than keeping a second palette. Alt: Dark/Light only (drops the
   media query but adds a decision for every user on first run).
4. **Tree state in the `settings` table under `files.<id>`**, restored
   only when "Restore layout on launch" is on. Alt: a new table (schema
   bump), or a per-project file (pollutes the repo).
5. **No Save all.** Alt: save every stashed draft from the modal — new
   store path, conflict UI inside a modal.
6. **Clamp shared as `src/menu.ts`**, two callers. Alt: leave the Sidebar
   as is.
