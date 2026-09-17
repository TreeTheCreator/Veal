# Veal — plan for step 8: file watching

Status: APPROVED 2026-09-17 (all decisions as recommended).

Scope: the app notices what shells and agents do to a pinned project's
files. Every pinned root is watched recursively with `notify`; changes are
coalesced in Rust into one `fs-changed` event per project per burst, and
JS acts only on what it is showing: an expanded directory re-lists, the
open file re-reads (a clean file updates in place, a dirty one gets the
conflict banner immediately rather than at `Ctrl+S`), and a project's
Tasks board reloads when its `.veal/tasks.json` changes. Nothing else
moves: no file create / rename / delete from the tree, no agent
auto-detection, no schema change. The Refresh buttons stay as the manual
fallback (decision 8).

## Dependencies

- **`notify = "8"`** (Rust). 8.x is the current stable line; 9 is at rc.
  `RecommendedWatcher` is `ReadDirectoryChangesW` on Windows — one handle
  per root, recursive, kernel-buffered — FSEvents on macOS, inotify on
  Linux. Windows is the target; the others are noted where they differ.

Nothing new on the JS side: `listen` from `@tauri-apps/api/event` is what
`pty.ts` already uses for `pty-exit`, and `core:default` covers it.

Considered and rejected:
- **`notify-debouncer-full`** — does path-level dedup, rename pairing and
  a file-id cache we do not need; the coalescer below is ~60 lines and its
  batching shape (dir-level, capped) is ours to test.
- **Polling (`PollWatcher` or a JS interval over `fs_list`)** — a 2 s
  poll of every expanded directory plus the open file plus N task files is
  more IPC per minute than a busy watcher, and still misses edits between
  ticks. Only useful on network drives, where `ReadDirectoryChangesW` is
  unreliable — see decision 7.
- **Watching only the active project.** The Tasks view in global scope
  shows every project's board, and switching projects would need
  stop/start plus a refresh on every switch. One handle per pinned root is
  cheap; the *filtering* is what keeps it quiet, not the handle count.
- **Sending every event path to JS.** `pnpm install` produces tens of
  thousands of events; after the `.git` filter and dedup the batch is
  capped (decision 4) and JS refreshes what it shows rather than reading
  the list.
- **Reloading the open file with a `loading` flip.** The `Editor`
  unmounts during `loading` (plan-06 findings), so every external write
  would blank the viewer for a frame. Re-reads from the watcher are quiet
  (below).

## Rust (`src-tauri/src/watch.rs`)

Tauri-agnostic like `fs.rs` / `tasks.rs`: the coalescer is a pure function
over a `Vec` of raw events, and the watcher registry takes a `sink`
closure, so `cargo test` covers both with a tempdir and no `AppHandle`.

```rust
pub struct Watchers { inner: Mutex<HashMap<u32 /* project id */, (PathBuf, RecommendedWatcher)>>, tx: Sender<Raw> }

struct Raw { project: u32, root: PathBuf, paths: Vec<PathBuf>, rescan: bool }

#[derive(Clone, Serialize)]
pub struct Batch {
    pub project_id: u32,
    /// Changed paths, absolute, deduplicated; `None` = too many or a rescan — refresh everything shown.
    pub paths: Option<Vec<String>>,
}
```

- `Watchers::new(sink: impl Fn(Batch) + Send + 'static)` spawns **one
  coalescing thread** for all roots. It blocks on `rx.recv()`, then
  drains with `recv_timeout(QUIET)` until the channel has been silent for
  `QUIET = 150 ms` or `MAX_LATENCY = 1000 ms` has passed since the first
  event of the burst — whichever comes first — then calls `sink` once per
  project id present in the burst. A running `pnpm install` therefore
  yields one batch a second, not one at the end.
- `coalesce(raws: &[Raw]) -> Vec<Batch>` (pure): groups by project;
  drops paths under `<root>/.git/` (the tree hides it; git's `index.lock`
  churn is the loudest thing on disk) and paths ending in `.veal-tmp` or
  `tasks.json.tmp` (our own atomic-write temps; the rename to the real
  name is what matters); deduplicates; if any raw had `rescan` (notify's
  `Rescan` flag, which `ReadDirectoryChangesW` raises on buffer overflow)
  or more than `CAP = 256` distinct paths survive, `paths` is `None`.
- `watch(project, root)`: `RecommendedWatcher::new` with a handler that
  maps `Event { paths, flag }` to a `Raw` and sends it; `RecursiveMode::
  Recursive`. Errors from the handler (`Err(notify::Error)`) become a
  `rescan` raw. A failed `watch` returns the error string — the caller
  toasts it (below) and the project simply is not watched.
- `unwatch(project)`: drops the watcher, which closes the handle.
- `roots()` for tests.

`lib.rs`:

- `setup` builds `Watchers` with `sink = move |b| app.emit("fs-changed",
  b)`, manages it, and watches every root in `db.projects_list()`. A root
  that fails to watch at boot emits `fs-watch-error { project_id, error }`.
- `projects_add` watches the new root after the insert (an error is
  returned as the `fs-watch-error` event, not as a command failure — the
  pin succeeded); `projects_remove` unwatches first.
- Event names join `pty-exit` as the app's second and third events. The
  payload is `serde` `rename_all = "camelCase"` so JS reads `projectId`.

Windows notes for the acceptance run: `ReadDirectoryChangesW` reports the
path as it was written, so a directory renamed over a watched subtree
arrives as one `Rename` for the directory (JS re-lists its parent); the
64 KiB kernel buffer overflows only under sustained thousands-per-second
churn and then sets `Rescan` → `paths: None`. On macOS, FSEvents may
report the parent directory rather than the file; the JS routing below
handles a directory path the same way (re-list it if expanded), so
nothing breaks, it just re-lists a little more.

## Frontend

### `src/watch.ts`

One `listen<Batch>("fs-changed", …)` started from `boot()` after the
stores are loaded, fanning out to `useFiles.getState().onChanged(b)` and
`useTasks.getState().onChanged(b)`; and one `listen("fs-watch-error", …)`
→ `showToast("Not watching <name>: <error> — use Refresh")` (name from
`useProjects`). Started once, like `startLayoutPersistence()`.

### `src/files/watch.ts` (pure, vitest)

```ts
export interface Affected { dirs: string[]; file: boolean }
export function affected(p: ProjectFiles, paths: string[]): Affected
```

For each changed path: its **parent** is re-listed if the parent is the
root or an expanded directory that has been listed (`dirs.has(parentKey)`
— an unexpanded directory nobody has looked at is left alone); the path
**itself** is re-listed if it is an expanded directory (macOS parent-path
events, or a directory the user has open that was renamed back). `file`
is true when a path's key equals the open file's key. Directories are
deduplicated and returned as real paths (the rows' paths, not the folded
keys) so `list` can be called with them; the root's real path comes from
`p.root`.

### `src/files/store.ts`

- `onChanged({ projectId, paths })`: no slice for the project → no-op.
  `paths === null` → `refresh(projectId)`. Otherwise `affected(p, paths)`
  → `list` each dir and, if `file`, a **quiet re-read** of the open file.
- `read` gains `{ quiet }`: a quiet read never sets `{ status: "loading"
  }`. On success it compares the fresh `mtime` and `size` with the loaded
  state and **does nothing when both match** — this is what swallows the
  event our own `fs_write` raises, so a save does not rebuild the editor
  and lose the cursor. When they differ: no draft → replace as `loaded`
  (new `gen`, editor rebuilds; if the file was in edit mode but clean it
  stays in edit mode, cursor at top); dirty draft → `conflict:
  "changed"` exactly as today. A `NotFound` error on a quiet read → dirty
  → `conflict: "deleted"`; clean → `file: { status: "error" }` and the
  tree row is gone by the parent re-list.
- `refresh` uses the quiet read too, so the button no longer blanks the
  viewer. `list` already keeps the previous entries during a re-list.
- No new state. The Refresh button and its title are unchanged.

### `src/tasks/store.ts`

- `onChanged({ projectId, paths })`: the project must be in `byProject`
  (loaded at least once — a project whose board was never shown is not
  read speculatively). `paths === null`, or any path whose key ends with
  `\.veal\tasks.json` or equals `<root>\.veal` (directory created or
  removed) → `load(projectId, root)` **without the `loading` flip** — a
  new `reload` internal that calls `apply(..., false)` directly, so the
  board does not flash. `root` comes from `useProjects` by id.
- Our own `tasks_*` writes raise an event too; the reload 150 ms later
  reads the same list and replaces it with an equal one. One small JSON
  read; no suppression (it would need an mtime the API does not return).
- `TasksView` keeps its "re-read on show" effect: the watcher covers
  changes while the view is showing and while it is hidden, the effect
  covers a project whose board was never loaded.

### `SettingsView`

A read-only row after **Task files**: **File watching** — "Every pinned
root, recursively; Refresh still re-lists by hand" · mono `notify`.

## Tests

- Rust `cargo test` (`watch.rs`):
  - `coalesce`: drops `<root>\.git\index.lock` and `a.txt.veal-tmp`, keeps
    `<root>\.gitignore`; dedups a path seen three times; 257 distinct
    paths → `paths: None`; a `rescan` raw → `None`; two projects in one
    burst → two batches.
  - Watcher integration (tempdir, `Watchers::new` with a channel sink):
    `watch(1, dir)`, write `dir/a.txt` → one `Batch { project_id: 1 }`
    containing `a.txt` within 2 s; write `dir/.git/x` → no batch within
    500 ms; `unwatch(1)`, write again → no batch. Marked `#[ignore]`-free
    but with generous timeouts; it runs on the dev machine, not CI.
  - Burst timing: 20 writes 10 ms apart → exactly one batch (quiet
    window), asserted on the sink's call count.
- vitest `files/watch.test.ts` (`affected`):
  - `src\App.tsx` with `src` expanded → `dirs: [src]`, `file: false`;
    `src` collapsed (never listed) → nothing; `x.md` at the root → `[root]`;
    the open file's own path → `file: true`; an expanded `src` itself
    (renamed) → `[root, src]`; three files in `src` → `[src]` once; case
    differences in the event path still match.
- vitest `files/store.test.ts`:
  - `onChanged` with the open file's path and an unchanged `mtime`/`size`
    → `read` called, `gen` unchanged, no `loading` state observed;
    changed `mtime` → new `gen`, new text; changed `mtime` on a dirty
    file → `conflict: "changed"`, text kept; `paths: null` → every
    expanded dir re-listed and the file re-read; unknown project → no
    calls.
- vitest `tasks/store.test.ts`: `onChanged` with `…\.veal\tasks.json` on a
  loaded project → `tasks.list` called, no `loading` state; on a project
  never loaded → no call; with `src\App.tsx` → no call; `paths: null` →
  reload.

## Acceptance (manual, dev window)

1. Files view on `veal`, `src` expanded. In a pane: `New-Item src\zz.ts`
   → the row appears within ~0.2 s, no Refresh. `Remove-Item src\zz.ts` →
   gone. `mkdir src\sub` → folder row; `Rename-Item src\sub src\sub2` →
   renamed.
2. Open `README`-sized file clean, in a pane `Add-Content <file> "// x"` →
   viewer shows the line, scroll at top, pill still `read-only`. Same with
   Edit on but no keystrokes → still in edit mode, text updated.
3. Edit a file, type, then `Add-Content` it from a pane → `conflict` pill
   and banner appear **without** pressing `Ctrl+S`; the draft is intact;
   Reload / Overwrite behave as in plan-06. `Remove-Item` it instead →
   `Deleted on disk.` banner.
4. Edit, type, `Ctrl+S` → cursor and scroll stay where they were (the
   self-write event is swallowed); `gen` unchanged in React devtools.
5. Tasks view on `veal`: in a pane edit `.veal\tasks.json` (flip a status
   with `sed`/editor) → the card moves within ~0.2 s. Switch to global
   scope, edit another project's file → its column updates. Click ▸ on a
   card → no flash (the board does not go to `loading`).
6. `pnpm install` (or `cargo build`) in a pane with `node_modules` /
   `target` **collapsed** → the app stays idle (Task Manager: WebView2 CPU
   flat), the tree does not churn. With `node_modules` expanded → the
   listing refreshes about once a second while the install runs, then
   settles.
7. `git commit` / `git checkout` in a pane → tree rows change for the
   files the checkout touched; no re-lists from `.git` traffic (add a
   `console.debug` in `onChanged` for the run, remove before commit).
8. Pin a project, its files change → watched immediately. Unpin it → in
   Task Manager `veal.exe` handle count drops by one (Sysinternals
   `handle -p veal` shows the directory handle gone).
9. Pin a path on a mapped network drive (or a `subst` drive) if one is
   handy: either it watches or a toast says `Not watching <name>` and
   Refresh still works.
10. Refresh button in Files: the open file does not blank while it
    re-reads.
11. `pnpm test`, `cargo test`, `pnpm build` green.
12. Commit: `Add file watching: notify per pinned root, live tree, open file and Tasks board`.

## Findings during acceptance (2026-09-17)

- Acceptance 1, 2, 5, 6, 7, 8 and 10 were run against the dev window over
  WebView2's remote-debugging port (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=
  --remote-debugging-port=9222`), driving the stores from a Node CDP script
  and touching files from the shell. `main.tsx` exposes the stores as
  `window.veal` in dev builds for this; production builds do not.
- **A burst that is all noise produced an empty batch** (`paths: []`) — the
  watcher test caught it; `coalesce` now emits nothing for a project whose
  burst was only `.git/` traffic or temp files.
- `git status` (which rewrites `.git/index`) produces no batch; a real write
  arrives once, tagged with the project id, with no `.git` paths.
- Pinning a directory watches it immediately; unpinning drops the handle
  (process handle count fell by 3) and the root goes silent.
- 400 files written under a collapsed `node_modules` left every listing
  untouched; the batch crossed IPC as `paths: null` (over the cap) and
  `refresh` re-listed only the root and `src`.
- Not run: 9 (no network drive to hand) and the boot-time
  `fs-watch-error` path; both are straight-line code.

## Decisions to confirm

1. **Watch every pinned root recursively, from boot, managed in Rust**
   (`setup` + `projects_add` / `projects_remove`); JS never starts or stops
   a watch. Alternative: JS-driven `watch_start` / `watch_stop` commands
   (explicit, but a boot race with the first `fs-changed`).
2. **Coalesce in Rust: 150 ms quiet window, 1000 ms maximum latency**, one
   `fs-changed` per project per burst. Alternative: emit raw events and
   debounce in JS (more IPC; the cap could not be applied before crossing).
3. **Filter in Rust: paths under `.git/` and our own `.veal-tmp` /
   `tasks.json.tmp` temps.** Nothing else — `node_modules` and `target`
   are not special; the JS routing ignores anything not on screen.
   Alternative: also drop `node_modules/` and `target/` (quieter, but an
   expanded `node_modules` would go stale).
4. **Cap 256 distinct paths per batch; over it, or on `Rescan`, the batch
   is `paths: null` and JS refreshes everything it shows for that project.**
   Alternative: no cap (a `pnpm install` batch of 30 k strings over IPC).
5. **JS acts only on what is displayed**: listed-and-expanded directories,
   the open file, a loaded Tasks board. Unexpanded directories and never-
   shown boards are not read. Alternative: eagerly re-list every cached
   directory (keeps collapsed-then-reopened folders fresher; more reads).
6. **Quiet re-reads: no `loading` flip, and an unchanged `mtime` + `size`
   is a no-op** — this is what keeps the cursor on `Ctrl+S`. Refresh
   adopts the same behaviour. Alternative: track "my last write's mtime"
   and skip only that (more state, same result).
7. **A root that cannot be watched is toasted once and left unwatched; no
   settings toggle, no polling fallback.** Refresh covers it. Alternative:
   a `watch_files` setting, or fall back to `PollWatcher` at 2 s for that
   root.
8. **Refresh buttons stay** in Files and Tasks. Alternative: remove them
   now that the watcher exists (they are the fallback for decision 7).
9. **Own task writes are not suppressed** — the reload 150 ms after a
   `tasks_*` command reads an equal list. Alternative: return `mtime` from
   the task commands and compare (API change for a 2 KB read).
10. **Settings gets a read-only "File watching" row**, like "Task files".
    Alternative: no row.
