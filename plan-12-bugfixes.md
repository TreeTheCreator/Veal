# Veal — plan for step 13: five bugs from the codebase review

Status: BUILT 2026-09-17 — written and built in one go at the user's
request. Each fix takes the smallest change that removes the bug; the
decisions at the end say what was not done.

Scope, the five findings ranked highest in the 2026-09-17 review:

1. **npm-installed Claude Code is not detected** (`agents.rs`). The npm
   shim runs `node …\node_modules\@anthropic-ai\claude-code\cli.js`; the
   script stem is `cli`, so `agent_of` says nothing. Plan-08 only had the
   native `claude.exe` to test against.
2. **A slow boot overwrites the saved Files tree with `[]`**
   (`files/persist.ts`). `ensure` seeds `expanded` (which arms the 250 ms
   save) before any listing has arrived; the save derives real paths from
   `flatten`, which cannot emit a directory whose parent is unlisted, so a
   root listing slower than the debounce writes an empty tree.
3. **Unpinning silently drops unsaved edits** (`Sidebar.tsx`,
   `projects/store.ts`). `remove` → `useFiles.forget` discards drafts with
   no prompt; delete and quit both ask first.
4. **`files.<id>` outlives its project and lands on the next pin**
   (`files/store.ts`, `db.rs`). `forget`'s early return (project never
   visited this session) skips the settings write; `projects.id` is
   `INTEGER PRIMARY KEY` without `AUTOINCREMENT`, so the next pin can get
   the same id and inherit the old tree and open file.
5. **`pty_write` blocks every pane under the registry mutex** (`pty.rs`).
   `write_all`/`flush` run while `panes` is locked, so one child that
   stops reading stalls every other pane's write, resize, kill and the
   agent poller.

## Dependencies

None.

## 1. Agent detection through package paths

`agents.rs`, `agent_of`: for an interpreter, a script argument now
matches by **package directory** as well as by stem. The path's
components after its last `node_modules` or `site-packages` component are
package components; one that equals an agent name, or is one of the known
package names (`claude-code` → claude, `gemini-cli` → gemini), decides.
Anything before `node_modules` is the user's own tree and never matches,
so `node C:\dev\claude\build.js` stays a shell. The stem rule is unchanged
and checked first.

Tests: `…\node_modules\@anthropic-ai\claude-code\cli.js` → claude;
`…\node_modules\@openai\codex\bin\codex.js` → codex (both rules agree);
`…\site-packages\aider\__main__.py` → aider; `C:\dev\claude\build.js` and
`…\node_modules\vite\bin\vite.js` → none.

## 2. Save the tree only once it can be flattened

`files/persist.ts`: the subscriber also re-arms on a `dirs` change, and
the save is skipped while it would be lossy — some key in `expanded` has
no row in `flatten` **and** some listing is still `loading`. Once every
listing has settled, a missing key means the directory is gone and is
dropped as before. The written string is remembered per project and an
identical one is not written again, so the extra triggers cost nothing.

Tests (`files/persist.test.ts`, `settledExpanded`): unlisted root +
loading → null (skip); unlisted root + nothing loading → `[]`; all
listed → the real paths.

## 3. Unpin asks about drafts

`Sidebar.tsx`: `unpin` first asks `useFiles.dirtyFiles()` for this
project; with any, a modal like FilesView's delete one — "Unpin <name>?
N files have unsaved changes." with the list, *Cancel* (autofocus,
Escape, scrim) / *Discard and unpin*. Without, it unpins as today.
`projects/store.remove` is unchanged: the modal is the only gate.

## 4. Rust owns the `files.<id>` row

`db.rs`, `projects_remove`: also `DELETE FROM settings WHERE key =
'files.<id>'`. `files/store.ts`, `forget`: drops `saved` and the in-memory
slice only — the settings write moves to Rust, where it cannot be
skipped. A debounced save that fires after `forget` finds no project and
returns, as it did.

Tests: Rust `remove_clears_files_key`; the JS `forget` test now asserts
no settings write.

## 5. Write outside the registry lock

`pty.rs`: `PtyHandle.writer` becomes `Arc<Mutex<Box<dyn Write + Send>>>`.
`write` clones the `Arc` under `panes`, releases it, then locks the
writer alone. Resize, kill and `pids` keep using `panes` as before, so a
blocked write only blocks other writes to the same pane.

No new test: a child that stops reading its pipe is awkward to build
portably, so the existing echo round-trip covers the write path and the
lock scope is reviewed, not measured.

## Acceptance (manual, dev window)

1. In a pane, `npx @anthropic-ai/claude-code` (or any node-shim CLI) →
   the badge turns red and names it; exit → shell.
2. Restart with a pinned repo whose root listing is slow (a network
   share, or throttle `fs_list` with a `sleep` in dev) → the tree comes
   back expanded on the *next* restart too; `files.<id>` in `veal.db`
   never holds `"expanded":[]` while dirs are shown.
3. Edit a file, don't save, right-click the project → Unpin → the modal
   lists it; Cancel keeps the pin and the draft; Discard and unpin
   removes both.
4. Pin B, expand dirs, quit, relaunch, unpin B without opening Files, pin
   C → `SELECT key FROM settings` has no `files.<B's id>` row and C's tree
   starts collapsed.
5. `pnpm test`, `cargo test`, `pnpm build` green.
6. Commit: `Fix npm agent detection, tree-save race, unpin guard, stale files key, PTY write lock`.

## Decisions (simplest option each)

1. **Package-component matching after `node_modules`/`site-packages`**
   with two package aliases. Alt: match any path component (false
   positives on the user's own directories); read `package.json` next to
   the script (a file read per process per tick).
2. **Skip the lossy save; re-arm on `dirs`.** Alt: persist the folded
   keys instead of real paths (changes the on-disk format and the keys
   are lowercase, useless on a case-sensitive FS); keep the restored
   paths as a fallback (two sources of truth).
3. **A modal in the Sidebar**, same shape as the delete one. Alt: refuse
   to unpin while dirty (a toast); save all (plan-11 decision 5 again).
4. **Delete the row in Rust.** Alt: `AUTOINCREMENT` (table rebuild
   migration, and the stale row would still exist); fix only the JS
   branch (the write can still be lost to a crash).
5. **Per-handle writer mutex.** Alt: a write thread per pane with a
   channel (more threads, more code); `try_lock` with an error (drops
   input).
