# Veal — plan for step 6: Tasks view

Status: APPROVED 2026-09-17 (all eleven decisions as recommended).

Scope: the Tasks placeholder becomes the mockup's three-column board
(To do / In progress / Done) with the scope segment (this project / all
projects), an add-task form, and ▸ / ↺ / × per card. Project tasks live in
`.veal/tasks.json` at each project root — the source of truth, git-trackable,
readable and writable by an agent in a pane. Tasks with no project go to an
**Inbox** in `veal.db`. Nothing else moves: no changes to `pty.rs`, `fs.rs`,
the layout store or the sidebar. This closes the spec's last open decision
("SQLite only, per-project files only, or both"): **both, partitioned** —
project tasks are files, Inbox tasks are rows, and SQLite never caches the
files.

## Dependencies

None new. `serde_json` (already a dependency) parses and writes the files;
the Inbox reuses `rusqlite`. Nothing on the JS side.

Considered and rejected:
- **SQLite as a cache/index over the files** (what the mockup's toolbar note
  says). A cache needs invalidation, and the files are the thing agents and
  `git checkout` change behind our back. Global scope reads every pinned
  project's file instead — a handful of files of a few KB each, on demand.
  The toolbar note becomes `N project files + Inbox`.
- **`chrono` for the `created` date.** `std` cannot produce a local date; the
  webview can. JS passes `YYYY-MM-DD` to `tasks_add` (decision 7).
- **`notify` (file watching).** Still deferred (plan-04 decision 6). Instead
  the view re-reads its scope every time it is shown (decision 8).
- **`rand` / `uuid` for ids.** Eight hex chars of the current nanosecond
  clock, regenerated on collision within the file, is enough for a file that
  holds tens of tasks (decision 6).
- **Mutating in JS with `tasks_read` / `tasks_write` commands.** A crash or
  external edit between the read and the write clobbers the file. Every
  mutation is one Rust read-modify-write under a lock (decision 4).

## File format (`<root>/.veal/tasks.json`)

```json
{
  "version": 1,
  "tasks": [
    { "id": "1a2b3c4d", "title": "Split-tree layout model", "status": "doing", "created": "2026-09-15" },
    { "id": "5e6f7a8b", "title": "Resize PTY on divider drag", "status": "todo", "created": "2026-09-15" }
  ]
}
```

- `status` ∈ `todo` | `doing` | `done`. `created` is a local date. Array order
  is display order within a column; new tasks append; a status change keeps
  the task's position.
- **One task per line**, two-space indent, trailing newline, always — so a
  git diff of the file is one line per changed task. `serde_json`'s pretty
  printer would spread each task over six lines; a ten-line formatter in
  `tasks.rs` writes the layout above instead.
- **Unknown fields round-trip.** `Task` and the document carry
  `#[serde(flatten)] extra: Map<String, Value>`, so a `"notes"` an agent adds
  to a task, or a top-level `"owner"`, survives our rewrite. Known keys are
  written first, extras after, alphabetically.
- The file is created on the first add, never on pin; the `.veal` directory
  with it. Deleting the last task leaves `"tasks": []` — a file is never
  removed.
- Rejected, with the message shown in the board and every mutation for that
  project refused until Refresh finds it fixed: JSON that does not parse
  (`tasks.json: <serde_json message with line:col>`), `version` other than 1
  (`tasks.json is version 2; this Veal reads version 1`), a status outside
  the three (`task "t3": unknown status "blocked"`), a duplicate id. **A file
  that failed to parse is never overwritten.**

## Rust (`src-tauri/src/tasks.rs`)

```rust
#[derive(Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
pub enum Status { Todo, Doing, Done }

#[derive(Serialize, Deserialize)]
pub struct Task { id: String, title: String, status: Status, created: String,
                  #[serde(flatten)] extra: Map<String, Value> }

#[derive(Serialize, Deserialize)]
struct TasksFile { version: u32, tasks: Vec<Task>, #[serde(flatten)] extra: Map<String, Value> }
```

Free functions on a root `&Path`, all `Result<Vec<Task>, String>` so the
store can replace its list with what came back:

| Function                          | Behaviour                                                                 |
|-----------------------------------|---------------------------------------------------------------------------|
| `list(root)`                      | Missing file → `Ok(vec![])`, no file created. Otherwise parse + validate. |
| `add(root, title, created)`       | Read → push `{ id: fresh, title, status: todo, created }` → write.        |
| `set_status(root, id, status)`    | Read → find by id (`no task "…"` if gone) → set → write.                  |
| `remove(root, id)`                | Read → filter → write.                                                    |

- **Write is atomic**: `.veal/tasks.json.tmp` then `rename` over the target
  (`std::fs::rename` replaces on Windows). `create_dir_all(.veal)` first.
- **Read-modify-write is serialised** by one `Mutex<()>` in managed state
  (`TasksLock`), so two quick clicks cannot interleave. Operations are by id,
  so an edit an agent made between two of our writes is merged, not lost;
  only an edit inside the ~1 ms window of one command could be.
- `title` is trimmed; empty after trimming → `Err("empty title")`.

Four commands in `lib.rs`, each taking `root: Option<String>`: `Some` is a
project root, confined via `fs::confine` like the Files commands; `None` is
the Inbox and dispatches to `Db`:

| Command             | Args                             |
|---------------------|----------------------------------|
| `tasks_list`        | `root`                           |
| `tasks_add`         | `root, title, created`           |
| `tasks_set_status`  | `root, id, status`               |
| `tasks_remove`      | `root, id`                       |

### Inbox (`db.rs`, schema v2)

```sql
CREATE TABLE tasks (
  id       TEXT PRIMARY KEY,
  title    TEXT NOT NULL,
  status   TEXT NOT NULL,
  created  TEXT NOT NULL,
  position INTEGER NOT NULL
);
```

`SCHEMA_VERSION` → 2; `migrate` adds the table when `version < 2`. Existing
`projects` / `settings` rows are untouched. `Db::inbox_list / inbox_add /
inbox_set_status / inbox_remove` mirror the file functions and return the
same `Vec<Task>` (ids from the same generator, `extra` empty), so the
frontend has one `Task` type for both.

## Frontend

### `src/tasks/api.ts`

`invoke` wrapper (`tasks.list(root)`, `.add(root, title, created)`,
`.setStatus(root, id, status)`, `.remove(root, id)`) plus the `Task` /
`Status` types. Lives under `src/tasks/` rather than as `src/tasks.ts`
because `./tasks` would then resolve to the file, not the directory.

### `src/tasks/board.ts` (pure, vitest-tested)

```ts
type Scope = "project" | "global";
interface Source { project: Project | null /* null = Inbox */; tasks: Task[] }
interface Card { task: Task; project: Project | null; chip: string | null }
function columns(sources: Source[], scope: Scope): Record<Status, Card[]>
function today(): string   // local YYYY-MM-DD
```

`columns` walks sources in the order given (sidebar order, Inbox last),
distributes each task to its status column, and sets `chip` to the project
name in global scope, `Inbox` for Inbox tasks in either scope, otherwise
`null` — the mockup's rule.

### `src/tasks/store.ts` (zustand)

```ts
type ListState = { status: "loading" | "loaded"; tasks: Task[] } | { status: "error"; tasks: Task[]; error: string };
interface TasksState {
  scope: Scope;                              // not persisted; opens on "project"
  byProject: Map<number, ListState>;
  inbox: ListState;
  setScope(scope): void;
  load(projectId: number | null, root: string | null): Promise<void>;
  add(projectId, root, title): Promise<void>;
  setStatus(projectId, root, id, status): Promise<void>;
  remove(projectId, root, id): Promise<void>;
  forget(projectId): void;
}
```

- `load` marks the list loading (keeping the old tasks so the board does
  not blank), then replaces it with the result or an error.
- Each mutation calls the command and replaces the list with the returned
  tasks — no optimistic update; the round trip is a local file write.
  An error (`no task "…"`, a parse error) lands in `ListState.error` and
  triggers a `load` so the board shows what is really on disk.
- `forget` is called from `projects.remove`, next to `useFiles.forget`.

### `TasksView` (`src/components/TasksView.tsx`)

Mockup `#view-tasks`: toolbar · `.board` · `.add-task` form.

- **Toolbar**: scope `.seg` (`<active name>` / `All projects`; the project
  button is disabled and global is forced when nothing is pinned) ·
  `.storage` note — project scope `~\veal/.veal/tasks.json · git-tracked`
  (`· git-tracked` in `--ok`), global scope `3 project files + Inbox` ·
  **Refresh** · grow · hint `▸ moves a task forward`.
- **Loading**: when `visible` becomes true, or the active project or scope
  changes while visible, `load` runs for the scope — the active project's
  file, or every pinned project's file plus the Inbox. Files are small and
  agents write them; re-reading on every show keeps the board honest without
  a watcher.
- **Board**: three `.tasks-col`s with `.col-head` label + mono count. Cards
  are `.task` (`.done` strikes the title): title, `.meta` row with the chip
  (per `columns`) and `.acts` — `▸` (todo → doing → done) or `↺` on done
  (→ todo), and `×` (delete, no confirm — mockup). Empty scope →
  `.empty` "No tasks yet — add one below."
- **Errors**: an unreadable file shows as a strip under the toolbar,
  `veal: tasks.json: expected `,` at line 4 column 7`, one line per bad
  file; its tasks are absent and, in project scope, the form is disabled
  with the hint "Fix tasks.json, then Refresh".
- **Add form**: text input `New task…` · `<select>` of every pinned project
  plus `Inbox` (preselects the active project; Inbox when nothing is pinned)
  · **Add**. Enter submits; blank does nothing. After a successful add the
  input clears and keeps focus; toast `Written to ~\veal/.veal/tasks.json`
  or `Saved to Inbox (veal.db)`. Adding to a project other than the active
  one while in project scope flips the scope to global so the new card is
  visible (mockup behaviour).
- `Ctrl+1/2/3` pass through as in the other views; the text input does not
  intercept them.

### `App.tsx` / `SettingsView.tsx` / `projects/store.ts`

The Tasks `<section>` body becomes `<TasksView visible={view === "tasks"} />`.
The Settings "Task files" row loses "(step 6)". `projects.remove` calls
`useTasks.getState().forget(id)`.

### `styles.css`

Ports the mockup's `.storage`, `.board`, `.col-head`, `.tasks-col`, `.task`,
`.chip` (+ `.inbox`), `.add-task` rules; `.seg` is already there. Adds
`.tasks-error` for the strip (mono 11.5 px, `--muted`, padded like the
toolbar).

## Tests

- Rust `cargo test` (`tasks.rs`, tempdir):
  - `list` on a root with no `.veal` → empty, and no directory appears.
  - `add` twice → the file's exact bytes match a literal (one task per line,
    trailing newline); ids differ; `.veal` was created.
  - `set_status` / `remove` round-trip and keep array order.
  - A hand-written file with `"notes"` on a task and `"owner"` at the top
    level survives `set_status` with both fields intact.
  - Malformed JSON → `Err` mentioning the line, and the bytes on disk are
    unchanged after an attempted `add`; `"version": 2` → `Err`;
    `"status": "blocked"` → `Err` naming the task; duplicate ids → `Err`.
  - `set_status` on an unknown id → `Err("no task …")`.
  - Id collision: `add_with_ids(root, …, iter)` (the internal form `add`
    calls with the clock) is given `["aaaa", "aaaa", "bbbb"]` and skips the
    duplicate.
- Rust `cargo test` (`db.rs`): opening a v1 database lands at
  `user_version = 2` with its projects and settings intact; Inbox add /
  set_status / remove / list keep position order.
- vitest `tasks/board.test.ts`: columns preserve source order (veal, hungy,
  Inbox); counts per column; chip is `null` in project scope for project
  tasks, the project name in global, `Inbox` for Inbox tasks in both;
  `today()` is ten characters `YYYY-MM-DD`.
- vitest `tasks/store.test.ts` (mocked api): `load` keeps stale tasks while
  loading; `add` replaces the list with the command's result; a rejected
  mutation records the error and reloads; `forget` drops the project;
  Inbox is `root === null` on every call.

## Acceptance (manual, dev window)

1. `Ctrl+3` with `veal` active and no `.veal` dir → three empty columns,
   storage note `~\veal/.veal/tasks.json · git-tracked`, form preselects
   `veal`. `ls .veal` in a pane → no such directory.
2. Type `Wire up tasks` + Enter → card in To do, count `1`, toast
   `Written to ~\veal/.veal/tasks.json`; `cat .veal\tasks.json` in a pane
   shows the two-line format above; `git status` shows `.veal/` untracked.
3. `▸` → In progress; `▸` → Done (struck through, `↺`); `↺` → To do; `×` →
   gone, file has `"tasks": []`.
4. Add three tasks, then in a pane edit the file: change one status to
   `doing`, add `"notes": "from the shell"` to another, add a fourth task by
   hand with id `manual`. Refresh → the board matches; `▸` on the noted
   task → `notes` is still in the file.
5. Break the file (delete a comma) → Refresh → error strip names the line,
   form disabled; fix it → Refresh → board back, file never rewritten while
   broken (compare `git diff` / mtime).
6. Set the file to `"version": 2` → refused with the version message.
7. Select `Inbox` in the form, add `Renew domain` → toast
   `Saved to Inbox (veal.db)`; project scope hides it, `All projects` shows
   it with a dashed `Inbox` chip, and project cards show their project chip.
8. In project scope with `veal` active, add a task to `hungy` → scope flips
   to global and the new card sits under a `hungy` chip; switch active to
   `hungy` → project scope shows only that task.
9. Files view → Refresh → `.veal/tasks.json` appears in the tree and opens
   as JSON in the viewer.
10. Unpin `hungy` → its tasks vanish from global scope; re-pin → they are
    back (the file was never touched).
11. Unpin every project → scope forced to global, project button disabled,
    form offers only `Inbox`.
12. `pnpm test`, `cargo test`, `pnpm build` green; a fresh `veal.db` from
    step 5 opens with its projects intact at schema v2.
13. Commit: `Add Tasks view: per-project .veal/tasks.json board and SQLite Inbox`.

## Decisions to confirm

1. **Files are the source of truth; SQLite holds only the Inbox; no cache.**
   Global scope reads N files on show. Alternative: the mockup's cache/index
   in `veal.db` (adds invalidation for a file agents edit).
2. **Keep the Inbox** (mockup) for tasks with no project — one table,
   schema v2, ~60 lines. Alternative: drop it; the form's select has no
   blank option and the schema stays at v1.
3. **Format: `{ version, tasks[] }`, one task per line, unknown fields
   preserved.** Alternative: `serde_json::to_string_pretty` (six lines per
   task, noisier diffs) and drop `extra`.
4. **All mutations are Rust read-modify-write under one lock, atomic
   tmp + rename.** Alternative: JS owns the list and calls a single
   `tasks_write` (simpler, but clobbers external edits).
5. **Strict validation: a bad status, duplicate id, unknown version or
   unparsable JSON refuses the whole file and blocks writes to it.**
   Alternative: lenient — unknown statuses shown in To do, ids deduplicated
   on write.
6. **Ids: 8 hex chars of the nanosecond clock, collision-checked within the
   file.** Alternative: `uuid` v4 (a crate, 36-char ids in a hand-edited file).
7. **`created` is a local `YYYY-MM-DD` computed in JS and passed to
   `tasks_add`.** Alternative: `chrono` in Rust, or a UTC timestamp.
8. **Re-read the visible scope every time the Tasks view is shown, on
   active-project or scope change, and on Refresh.** No watcher. Alternative:
   read once per project like the Files tree, Refresh only.
9. **No title editing, no reordering, no ◂ back-step in step 6** — the
   mockup's ▸ / ↺ / × only. Edit the file for anything else. Alternative:
   double-click to edit the title now.
10. **Scope is not persisted; the view opens on "This project"** (global when
    nothing is pinned), like `ui.view`.
11. **No cross-store refresh of the Files tree after a write** — Files has
    its own Refresh. Alternative: call `useFiles.refresh(id)` after each
    tasks write.
