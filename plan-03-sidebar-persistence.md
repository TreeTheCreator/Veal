# Veal — plan for step 4: sidebar + projects + SQLite persistence

Status: APPROVED 2026-09-17 (all ten recommendations as listed).

Scope: the hard-coded `CWD` goes away. The sidebar lists pinned projects,
"new terminal" spawns in the active project's root, the top-bar tabs switch
views, and projects / settings / the pane layout persist in SQLite so the app
reopens where it left off. Files and Tasks views are empty placeholders
(steps 5 and 6). Steps 1–3 stay as they are: no changes to `tree.ts`,
`rects.ts` or `pty.rs` beyond passing the shell setting through.

## Dependencies

| Side | Package                    | Version | Purpose                                                    |
|------|----------------------------|---------|------------------------------------------------------------|
| Rust | rusqlite `bundled`         | 0.40.2  | SQLite compiled in; no system dll, no plugin capability surface |
| Rust | tauri-plugin-dialog        | 2.7.3   | native "Browse…" folder picker in the add-project modal (decision 4) |
| JS   | @tauri-apps/plugin-dialog  | 2.7.3   | JS side of the above                                       |

`bundled` compiles the SQLite C source with MSVC (~1 min extra on the first
build only). `tauri-plugin-sql` was considered and rejected: it exposes raw SQL
to the webview, needs capability entries per statement pattern, and is async
over sqlx for a database that will hold a few dozen rows.

## Database (`src-tauri/src/db.rs`)

Path: `app.path().app_data_dir()/veal.db`
→ `C:\Users\user\AppData\Roaming\com.veal.app\veal.db` (dir created on start).
One `Mutex<rusqlite::Connection>` in managed state, `journal_mode=WAL`,
`foreign_keys=ON`. Schema versioned with `PRAGMA user_version`; v1:

```sql
CREATE TABLE projects (
  id       INTEGER PRIMARY KEY,
  name     TEXT NOT NULL,
  path     TEXT NOT NULL UNIQUE,      -- absolute, trailing slash stripped
  position INTEGER NOT NULL           -- sidebar order; append = max+1
);
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

Settings keys (all TEXT):

| key              | values                                   | default |
|------------------|------------------------------------------|---------|
| `active_project` | project id                               | unset   |
| `default_kind`   | `shell` \| `agent`                       | `shell` |
| `shell`          | `auto` \| `pwsh` \| `powershell` \| `cmd` \| `wsl` | `auto` (pwsh → powershell, as today) |
| `restore_layout` | `1` \| `0`                               | `1`     |
| `layout`         | JSON `{ tree, focused, nextId }`         | unset   |

The layout is one JSON blob, not normalised tables: it is ≤ 4 leaves, it is
always read and written whole, and the tree type already serialises as-is.

### Commands (`Result<_, String>` like the PTY ones)

| Command                        | Args                    | Returns / notes                                                  |
|--------------------------------|-------------------------|------------------------------------------------------------------|
| `projects_list`                |                         | `Vec<Project>` ordered by `position`                             |
| `projects_add`                 | `name, path`            | `create_dir_all(path)` if missing, insert → `Project`; UNIQUE → `"already pinned: {path}"` |
| `projects_remove`              | `id`                    | `()`; clears `active_project` if it pointed here                 |
| `settings_get_all`             |                         | `HashMap<String, String>`                                        |
| `settings_set`                 | `key, value`            | `()` upsert                                                      |
| `path_kind`                    | `path`                  | `"dir"` \| `"file"` \| `"missing"` — feeds the modal's hint      |

`Project = { id: u32, name: String, path: String }`.

## Frontend

### Stores

`src/projects/store.ts` (zustand):
```ts
{ projects: Project[]; activeId: number | null; loaded: boolean;
  load(); add(name, path): Promise<Project>; remove(id); setActive(id) }
```
`setActive` writes `active_project` through immediately (one row, no debounce).

`src/settings/store.ts`:
```ts
{ defaultKind, shell, restoreLayout; load(); set(key, value) }
```

`src/ui.ts`: `{ view: "terminal" | "files" | "tasks" | "settings" }` — not persisted; the app always opens on Terminal.

### Layout persistence (`src/layout/persist.ts`)

- `useLayout.subscribe` → serialise `{ tree, focused, nextId }` → `settings_set("layout", …)`, **debounced 250 ms** (divider drags write weights on every `pointermove`). Worst case on a crash: the last 250 ms of a drag is lost. No flush-on-close: `CloseRequested` on the Rust side kills PTYs and exits; adding a JS round-trip there is not worth it for weights.
- Boot order in `App`: `settings.load()` + `projects.load()` → if `restore_layout === "1"` and `parseLayout(json)` succeeds → `useLayout.setState(...)`; else one shell pane in the active project's path (today's behaviour); if there is no project at all, empty layout with the hint "Pin a project to open a terminal".
- `parseLayout` validates shape, drops the tree if it has more than `PANE_CAP` leaves, and sets `nextId = max(id) + 1` so restored ids never collide.
- Restored panes respawn **fresh shells** in their saved `cwd` with their saved `kind`; scrollback is gone, which is the same as tmux-resurrect without a scrollback plugin. A `cwd` that no longer exists makes `pty_spawn` fail, and the pane already shows that error footer (step 2); close it with `×`. No special case.

### Sidebar (`src/components/Sidebar.tsx`, mockup `renderSidebar`)

Row = mockup's 4-column grid `16px 1fr auto auto`, padding 7×10, radius 8:
folder icon · name (14 px) over `tilde(path)` (mono 11 px `--muted`) · live dot · `+`.

- Click → `setActive(id)`. Active row: `--sel` background, name weight 500.
- `+` (visible on hover/active, title "New terminal here") → the existing type popover anchored to the row → `addPane(kind, project.path)`. Does **not** switch the active project (mockup does; spec says the multiplexer shouldn't care — and it keeps the sidebar click and the spawn click distinct).
- **Live dot**: 7 px, `--agent` with a 3 px `--agent-soft` ring, shown when `panes(tree).some(p => p.kind === "agent" && samePath(p.cwd, project.path))`. Computed from the spawn `cwd`, no `projectId` on `PaneNode` — the spec's "no special awareness of projects beyond each pane's own cwd". `samePath` normalises case and trailing separators (Windows).
- Right-click on a row → 1-item menu **Unpin**. No confirm; open panes in that project stay open (they own their PTY). Header count updates.
- `+ Add project` → modal (below). Settings button → `view = "settings"`, gets `.active` like a tab.

### Add-project modal (mockup `#addProjectModal`)

Fields: **Name** · **Root directory (optional)** with a **Browse…** button
(dialog plugin, `open({ directory: true })`, capability `dialog:allow-open`).
Derivations, from the mockup:

- blank root → `C:\Users\user\<name>` (`homeDir()` from `@tauri-apps/api/path`, already covered by `core:default`); hint "Leave blank to create it in ~"
- blank name → `basename(root)`
- both blank → focus Name, no submit
- root exists as a file → error under the field; `"already pinned"` from Rust → same
- submit → `projects_add` → new project becomes active → toast `Created ~\x and pinned it` / `Pinned ~\x` (mockup toast, 2200 ms)

`Escape` closes it; `Enter` submits.

### Views and tab switching

All four `<section class="view">` stay **mounted**, stacked in the same grid
cell; the inactive ones get `visibility: hidden; pointer-events: none`, not
`display: none`. Reason: `PaneLayout` unmounts its panes (and their PTYs) when
its `ResizeObserver` reports `0×0`, which is exactly what `display: none`
produces. With `visibility: hidden` the container keeps its real size, so
xterm's fit and the debounced `pty_resize` never see a bogus size either.

- Tabs: click, or `Ctrl+1/2/3`. xterm swallows key events when focused, so
  `term.attachCustomKeyEventHandler` returns `false` for `Ctrl+1/2/3` to let
  them reach the window listener.
- Files view: toolbar "Root `~\<active>`" + the mockup's empty-viewer text
  "Pick a file from the tree." — placeholder until step 5.
- Tasks view: toolbar with the scope segment (non-functional) + "Tasks arrive in step 6."
- Terminal toolbar: `+ New terminal` disabled when there is no active project;
  hint `Opens in ~\<active>` / cap message as today. The popover preselects `default_kind`.

### Settings view (mockup `#view-settings`, 580 px sheet)

| Row                        | Live in step 4?                                              |
|----------------------------|--------------------------------------------------------------|
| Appearance                 | **no** — app CSS is dark-only; the light tokens were never ported. Row omitted. |
| Default pane type          | yes → `default_kind`                                         |
| Shell                      | yes → `shell` select: Auto / pwsh / powershell / cmd / wsl; applies to new panes only |
| New projects go in         | read-only `~`                                                |
| Pane soft cap              | read-only `4` (`PANE_CAP`)                                   |
| Task files                 | read-only `.veal/tasks.json` — a statement of intent for step 6 |
| Restore layout on launch   | yes → `restore_layout`                                       |

Toolbar hint "Saved to veal.db". Every control writes through on change.

### PTY change

`pty_spawn` already takes `shell: Option<String>`; `TerminalPane` now passes
the setting (`auto` → `None`, else the exe name). Nothing else in `pty.rs` moves.

## Tests

- Rust `cargo test` (in-memory connection): migration lands at `user_version = 1`;
  add → list roundtrip; second add of the same path returns the `already pinned`
  error; remove clears `active_project`; settings upsert overwrites.
- vitest `persist.test.ts`: `parseLayout` round-trips the `(1/3) | (2/4)` tree;
  rejects malformed JSON, a leaf with an unknown `kind`, and a 5-leaf tree;
  `nextId` becomes `max(id) + 1` even if the saved `nextId` is lower.
- vitest `paths.test.ts`: `samePath` matches `C:\Users\user\veal\` ↔ `c:/users/user/veal`;
  add-project derivations (blank root → `~\name`, blank name → `basename`).
- vitest `sidebar.test.ts`: live-dot selector is true only for an `agent` pane
  whose cwd matches; a `shell` pane in the project does not light it.

## Acceptance (manual, dev window)

1. First launch: empty sidebar, "Pin a project to open a terminal", New terminal disabled. `veal.db` exists under `%APPDATA%\com.veal.app`.
2. Add project → Browse… picks `C:\Users\user\veal` → name auto-fills `veal` → Pin → row appears active, toast "Pinned ~\veal", New terminal enabled with hint "Opens in ~\veal".
3. Add project with name `scratch` and blank root → `C:\Users\user\scratch` is created and pinned; toast says "Created".
4. Hover `hungy` row (after pinning `Hungy games`) → `+` → Agent → pane opens in that path with a red stripe; the `hungy` row's dot lights up; the active project is unchanged. Flip the badge to shell → dot goes out.
5. `Ctrl+2` while a terminal is focused → Files placeholder; `Ctrl+1` back → all panes still there with their scrollback (no remount, no PTY respawn).
6. Open 3 panes across 2 projects, drag a divider, close the app, reopen → same tree, same weights, same cwds, fresh prompts; `Get-Process pwsh` count matches the pane count.
7. Settings → Shell = cmd → new pane is `cmd.exe`; existing panes untouched. Restore layout off → relaunch opens one shell pane in the active project.
8. Right-click `scratch` → Unpin → row gone, count decrements, any pane in it keeps running.
9. `pnpm test` and `cargo test` green.
10. Commit: `Add pinned-project sidebar, view switching and SQLite persistence`.

## Findings during acceptance (2026-09-17)

- **Restored panes stalled after 4 bytes.** ConPTY's conhost opens with a
  cursor-position query (`ESC [ 6 n`) and emits nothing further until the
  terminal answers. With several panes spawning at once, that query reached
  xterm before `pty_spawn` resolved, and `term.onData` was only attached after
  it resolved, so the reply was dropped. `TerminalPane` now wires `onData`
  before spawning and queues input until the session exists. UI-added panes
  only ever worked because the query happened to arrive after the invoke.
- **A pane whose saved cwd no longer exists did not fail to spawn** on
  Windows: `cmd`/`powershell` silently started in `C:\Users\user` instead.
  `PtyRegistry::spawn` now rejects a cwd that is not a directory
  (`no such directory: …`), so the pane shows the step-2 error footer as
  the plan intended; close it with `×`.
- A typed root with forward slashes (`C:/x/`) is canonicalised to `C:\x` on
  both sides so it matches a picked path and the live-dot comparison.

## Decisions to confirm

1. **rusqlite (bundled) via Tauri commands**, not `tauri-plugin-sql`. Recommended.
2. **Layout stored as one JSON blob** in `settings`, not normalised tables. Recommended.
3. **Live dot = agent pane whose spawn cwd equals the project root**; no `projectId` on panes. A pane the user `cd`s elsewhere still counts (we don't track the live cwd); a pane opened via the sidebar is always at the root anyway.
4. **Browse… via `tauri-plugin-dialog`** — one extra plugin and a capability line, in exchange for not typing Windows paths. Alternative: text field only, as in the mockup.
5. **Blank root creates `~\<name>`** (mockup behaviour). Alternative: require an existing directory.
6. **Unpin via right-click menu, no confirmation**, panes unaffected. Alternative: hover `×` on the row.
7. **First run is empty** (no auto-pinned `~\veal`). Alternative: seed the current repo as the first project for dev convenience.
8. **Shell options** Auto / pwsh / powershell / cmd / wsl; applies to new panes only.
9. **Appearance (theme) row deferred** until light tokens are ported.
10. **Layout save debounce 250 ms**, no flush on close.
