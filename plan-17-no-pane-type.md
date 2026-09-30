# Veal — plan for step 18: New terminal opens a shell, no pane type

Status: sections 1–6 BUILT 2026-09-30, not yet committed.

Results: `pnpm test` 167 → 167 (the baseline already had the uncommitted
Ctrl+V paste test; −1 flip, +1 legacy kind), `cargo test` 58, `tsc`
clean, `pnpm build` and `pnpm tauri build` green. Acceptance 1–6 pass on
the dev window over CDP, and the legacy restore plus one-click spawn pass
again on the release exe. Step 4: red with a `claude` badge and the dot lit
3.1 s after launch, then blue with `shell` 1.8 s after exit. Beyond the
plan: `tsc` found `addPane("shell", …)` in `App.tsx`'s boot seed,
`store.test.ts` and `persist.test.ts`. The toolbar hint "click a badge to
flip agent ⇄ shell" went too. Step 2's "the toast shows" at the cap does
not hold, either before or after this change. The `noSpawn` guard stops
the click before `addPane`, so the only cap signal is the `+` title
("Soft cap of 4 reached").

Scope: **+ New terminal** and the sidebar's `+` open a pane straight away,
with no Shell / Agent popover. The manual pane type is removed completely:
it disappears from the layout tree, the saved snapshot, the settings and
the pane header. A pane's colour comes only from agent detection (plan-08).
It is blue with a `shell` badge, and red with the agent's name while a
detected agent runs under its shell. Nothing moves in Rust. `pty_spawn`
already starts the same shell for both types, and detection already ignores
the label.

Why: the label changes nothing about what runs. Everything else in the app
already goes by detection: the accent and badge while an agent runs
(`TerminalPane.tsx:39`), the sidebar dot (`liveProjectIds`) and the close
guards (`runningAgents`, plan-16 decision 4). The label only matters when
no agent is running, and then it is wrong. An idle pane labelled Agent
shows red, and a restored layout comes back red with nothing running. This
closes plan-08's "manual label, auto-detected truth" as **detected truth
only**.

## Dependencies

None added, none removed.

## 1. The layout tree has no kind

`layout/tree.ts`: delete `PaneKind` and `setKind`. `PaneNode` becomes
`{ type: "pane"; id; cwd }`.

`layout/store.ts`: `addPane(kind, cwd)` → `addPane(cwd)`. Delete
`flipKind`.

## 2. Old snapshots still load

`layout/persist.ts` `parseNode`: stop reading `kind`. A saved pane with
`kind: "shell"`, `"agent"` or anything else parses. The field is dropped,
so the next save writes panes without it. The id and cwd checks stay as
they are. A layout saved before this change restores all its panes, and a
pane labelled Agent comes back blue unless an agent is detected in it.

Rejected: keeping the check that `kind` is `shell` or `agent`. It would
make the parser refuse snapshots for a field the app no longer has, for no
benefit.

## 3. One click spawns

`NewTerminalButton.tsx`: a plain button. `onPick(kind)` → `onNew()`, which
calls `addPane(active.path)` in `App.tsx`. `disabled` and the `+ New
terminal` text stay as they are. The open state and `data-popover-anchor`
go.

`Sidebar.tsx`: the `+` calls `spawn(p)` directly on click and on
Enter/Space. The `noSpawn` guard, the `spawnTitle` text and the soft-cap
toast stay. `spawnFor`, `toggleSpawn`, `data-popover-anchor` and the
`useSettings` read go.

`TypePopover.tsx`: deleted. `styles.css`: delete the `.popover` block
(lines 93–99) and `.popover.side` (184). `TypePopover` was their only
user.

## 4. The pane header

`TerminalPane.tsx`:
- The accent becomes `agent ? "agent" : "shell"`.
- The badge becomes a `<span>` that reads `agent ?? "shell"`, with title
  `${agent} running` when an agent is detected and no title otherwise.
- `onFlip` goes from the props, and from `PaneLayout.tsx:76`.
- `styles.css`: delete `.badge.flip:hover`.

The `.pane.shell` / `.pane.agent` and `.badge.shell` / `.badge.agent`
rules stay unchanged, because they are now driven by detection alone.

## 5. Settings

`settings/store.ts`: delete `defaultKind`, `setDefaultKind` and `isKind`,
and the `default_kind` read in `load()`. `SettingsView.tsx`: delete the
"Default pane type" row. The "Agent detection" row stays.

The `default_kind` row already in `veal.db` stays and is never read.
Deleting it would take a new `settings_delete` command for one unused
key.

## 6. The spec

`veal-spec.md`, the "Pane types" bullets become: a pane is blue with a
`shell` badge; while a known agent CLI runs under its shell it is red and
the badge names the agent. Remove "set at creation … worth deciding". The
header bullet's "type badge" stays.

## Tests

- `tree.test.ts`: delete "flips one pane and shares untouched subtrees"
  (−1). Rename the describe to `setWeights`. Drop `kind` from the `pane`
  helper.
- `persist.test.ts`: in "rejects malformed input", replace the
  `kind: "robot"` line with nothing, since unknown kinds now parse. Add
  "drops a legacy kind" (+1): a snapshot with one pane at `kind: "agent"`
  and one at `kind: "robot"` parses to two panes without `kind`, and
  `serializeLayout` of the result has no `kind`. Drop `kind` from the
  helper.
- `rects.test.ts`, `live.test.ts`: drop `kind` from the helpers. The
  live cases that labelled a pane `agent` to show the label does not count
  lose that argument and keep their assertions.

Expected: `pnpm test` 166 → 166, `cargo test` unchanged at 58, `tsc`
clean. That `tsc` passes is the check that no `kind` / `PaneKind`
reference is left.

## Acceptance (CDP on the dev window, then the release exe)

1. Click **+ New terminal** → a pane opens at once, with no popover.
   `window.veal.useLayout.getState().tree` has no `kind` on it. There is
   one more `powershell.exe` under `veal.exe`.
2. Sidebar `+` on a project → a pane opens there at once. Tabbing to `+`
   and pressing Enter does the same. At the soft cap, `+` does nothing
   and the toast shows.
3. The new pane is blue, with the badge `shell` and no hover ring. The
   badge is not a button.
4. Run `claude` in it → within one tick (3 s) the pane is red, the badge
   reads `claude` and the sidebar dot lights. Exit → blue and `shell`
   again.
5. Before building, save a layout with one pane labelled Agent. After
   the change, relaunch → the pane restores, blue, same cwd. The next
   saved `layout` value has no `kind`.
6. Settings has no "Default pane type" row.
7. `pnpm test`, `cargo test`, `tsc`, `pnpm build`, `pnpm tauri build`
   all green.
8. Commit (user's step): `Open a shell on New terminal; drop the manual
   pane type`.

## Decisions (simplest option each)

1. **Remove the label, don't just hide the popover.** Alt: keep `kind`
   defaulting to shell and keep the flip badge. That leaves a control
   whose only effect is a red border that can be wrong.
2. **Legacy `kind` is ignored, not rejected.** Alt: bump the snapshot
   format and migrate. That is more code for a field the app simply drops.
3. **Leave the `default_kind` settings row in the database.** Alt: a
   `settings_delete` command, a Rust change for one dead key.
4. **The badge stays as a status label that is not clickable.** Alt:
   drop the badge when no agent runs. The spec's header lists a badge,
   and a steady `shell` keeps the header layout still.
