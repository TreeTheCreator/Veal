# Veal — plan for step 17: the 2026-09-20 pressure-test bugs

Status: sections 1–18 BUILT 2026-09-20, not yet committed.

Results for 6–18: `pnpm test` 166, `cargo test` 58 (+1 `cfg(unix)`
case-rename test, +1 ignored `poller_cost`), `tsc` clean, `pnpm build`
green. Section 7's measurement (release, idle, no build running, 40
refreshes at 1.5 s): p50 16.7 ms, max 25.9 ms, 1.1 % of a core — over the
5 ms rule, so `TICK` is now 3 s. Acceptance 6–15 run over CDP on the dev
window (10 is unix-only, skipped): three panes then F5 gave three
`powershell.exe` under `veal.exe`, all new pids; a dispatched right-click
was default-prevented on the terminal, header and sidebar row while the
sidebar menu still opened; root `foo` pinned as `~\foo` with the hint
`Will be created in ~`, root `..` showed `no .. in a project path`, nothing
under `src-tauri`; `C:\` gave `pin a folder, not a drive` and
`c:/users/USER/VEAL` gave `already pinned: C:\Users\user\veal`; a pane
closed and WM_CLOSE posted within ~50 ms was gone on relaunch (saved and
restored trees both `[9, 11]`); the release exe launched twice left one
window in the foreground, the second exiting 0 (and a release launch
beside the dev window focused the dev window); the pwsh setting gave
`failed to start shell: pwsh.exe is not on PATH`; a `Set-Content -Encoding
UTF8` tasks.json (PS 5.1, `EF BB BF`) listed and was rewritten without
the BOM; a pinned root renamed away struck the row through and disabled
`+` after the spawn failed, and a successful spawn after renaming it back
cleared it (`projects_list` said `missing: false` too).

Results for 1–5: `pnpm test` 160, `cargo test` 53, `tsc` clean. Dev
window over CDP: a junction's Delete removed the link and left the
target's file in place; a Latin-1 file reads `lossy: true` and its bytes
are unchanged after the interaction; Ctrl+C over a selection put the
selection on the clipboard and wrote nothing to the shell, Ctrl+C with
none interrupted `ping`; `path_kind` answered in 37–38 ms (three runs)
while a 2 MB `pty_write` was in flight (was 4735 ms); closing a pane
running Claude Code asked and Cancel kept it, WM_CLOSE on the window
listed `claude in veal` and Cancel kept the window, and the same pane
closed with no modal once Claude had exited.

Scope: the eighteen defects from the 2026-09-20 pressure test (static
read of every module, then the dev window driven over CDP with junctions,
non-UTF-8 files, a 2 MB paste, 3000-file storms, a second instance and
real Claude Code). Same rule as plans 12–14: the smallest change that
removes each bug. The UX findings from the same test (pane shortcuts,
the cap, a custom shell path, `-NoLogo`, right-click paste, a watcher
ignore list) are features, not bugs, and are not in this plan.

Ranked as in the report:

1. **Delete / rename on a junction acts on the target** (`fs.rs`,
   `lib.rs`). `confine` canonicalises, so `zz-link → zz-target` + Delete
   sent `zz-target` to the Recycle Bin and left a dangling junction;
   Rename renamed the real directory. Reproduced live.
2. **Non-UTF-8 files are corrupted on save** (`fs.rs`, `files/store.ts`,
   `CodeViewer.tsx`). `read` decodes lossily and nothing remembers it, so
   a no-op Ctrl+S turns `é` into `EF BF BD`. Reproduced live.
3. **Ctrl+C with a selection sends `^C`** (`TerminalPane.tsx`). xterm has
   no copy binding; the selection is dropped and the agent is
   interrupted. Reproduced live (`\u0003` written).
4. **Closing a pane or the window kills a running agent with no prompt**
   (`PaneLayout.tsx`, `CloseGuard.tsx`).
5. **Every command runs on the main thread, serialised** (`lib.rs`). A
   2 MB `pty_write` into a non-reading child took 4.8 s and
   `settings_get_all` waited 4.7 s behind it; a 3000-file Recycle-Bin
   delete blocked everything for 1.7 s. Reproduced live.
6. **A page reload orphans every shell, and right-click offers one**
   (`App.tsx`, `pty.rs`, `lib.rs`, `main.tsx`). React cleanup does not
   run on unload; WebView2's default context menu (not prevented
   anywhere) has "Refresh". 4 → 8 → 11 `powershell.exe` children
   reproduced live.
7. **Idle CPU from the 1.5 s process-table poll** (`agents.rs`). 3.4 % of
   a core measured, but during a cargo build — a measurement, not yet a
   fix.
8. **Relative project paths are created under the exe's cwd** (`db.rs`,
   `paths.ts`). Pinning `zz-rel-probe` created `src-tauri\zz-rel-probe`;
   `..` pinned the cwd's parent. From a Start-menu launch the cwd is
   System32.
9. **Duplicate pins by case; a drive root pins** (`db.rs`).
   `c:\users\USER\VEAL` beside `C:\Users\user\veal`; `C:\` watches the
   whole drive.
10. **Case-only rename clobbers on a case-sensitive filesystem**
    (`fs.rs`). `a.txt → A.txt` is allowed as "same entry" and
    `std::fs::rename` overwrites a distinct `A.txt` on Linux / WSL dirs.
11. **The layout save is debounced 250 ms with no flush on close**
    (`layout/persist.ts`, `files/persist.ts`, `CloseGuard.tsx`). Close a
    pane, hit ✕ within 250 ms → the pane is back next launch.
12. **No single-instance guard** (`Cargo.toml`, `lib.rs`). Two windows,
    same DB, duplicate shells, both writing `layout`.
13. **A missing shell shows a Rust debug string** (`pty.rs`):
    ``spawn pwsh.exe: CreateProcessW `"pwsh.exe\0"` in cwd `Some("…\0")` failed``.
14. **`tasks.json` with a UTF-8 BOM is refused** (`tasks.rs`) as
    `expected value at line 1 column 1` — what PowerShell 5.1's
    `Set-Content -Encoding UTF8` writes.
15. **A pinned root that is gone looks normal in the sidebar**
    (`db.rs`, `db.ts`, `projects/store.ts`, `Sidebar.tsx`,
    `TerminalPane.tsx`).
16. **`parseLayout` accepts `-5` and `1e300` as ids; `dragWeight(NaN)`
    yields `null`** (`layout/persist.ts`, `layout/rects.ts`).
17. **A thread-spawn failure leaks the pane; `inbox_shift` is not
    transactional** (`pty.rs`, `db.rs`).
18. **Name length counts UTF-8 bytes** (`fs.rs`, `files/names.ts`): 64
    emoji (128 UTF-16 units, legal on NTFS) is "too long".

## Dependencies

12 adds `tauri-plugin-single-instance = "2"`. Nothing else new.

## 1. Confine the entry, not what it points to

`fs.rs`: a second confinement, `confine_entry(db, path)`, for commands
that act on a directory entry rather than its contents: canonicalise the
**parent**, require it under a pinned root, then join the original
`file_name` and require `symlink_metadata` to succeed. The returned path
names the link itself. `fs_rename` and `fs_delete` switch to it;
`fs_list`, `fs_read`, `fs_write` and `fs_create` keep `confine` (reading
*through* a link inside the project is what a tree does, and a link that
leads outside every root still errors `outside pinned projects`, as
today). `rename` compares `path.file_name()` as before, so a case-only
rename of a link renames the link.

Tests (`cfg(windows)` uses `cmd /c mklink /J`, unix
`std::os::unix::fs::symlink`): `confine_entry` on a link returns the
link's path and its target is untouched; `rename` on a link renames the
link and the target stays; `confine_entry` on a link whose *parent* is
outside every root errors. Delete stays acceptance-only (plan-09's
Recycle-Bin reason).

## 2. A lossy read is read-only

`fs.rs`, `read`: decode with `String::from_utf8` first; on failure fall
back to `from_utf8_lossy` and set a new `FileText.lossy: bool`. `write`
is unchanged — it is never reached. `fs.ts` mirrors the field.

`files/store.ts`: `loaded(f)` keeps `lossy`; `setEditing(on = true)` is
ignored for a lossy file, the way it is ignored while dirty.
`CodeViewer.tsx`: the pill reads `not UTF-8` (agent colour) and the Edit
button is disabled with the title `Not UTF-8: shown lossily, cannot be
saved from Veal`. Files with a UTF-16 BOM still read as `binary` (their
NULs hit the sniff) — the message is enough; no decoder is added.

Tests: `read` of `caf\xE9` → `lossy: true`, text has U+FFFD;
`read` of plain UTF-8 → `lossy: false`; store: `setEditing` on a lossy
file leaves `editing` empty.

## 3. Ctrl+C copies when there is a selection

`ui.ts`: a pure `terminalKeyAction(e, hasSelection)` → `"copy" |
"view" | "pass"`: Ctrl+C (no Alt/Meta) with a selection, or
Ctrl+Shift+C with one, is `copy`; a view shortcut is `view`; else
`pass`. `TerminalPane.tsx`'s `attachCustomKeyEventHandler` uses it:
`copy` → `navigator.clipboard.writeText(term.getSelection())`,
`term.clearSelection()`, return `false` (xterm never sees it, no `^C`
goes to the shell). Ctrl+C with no selection is unchanged. Paste is
already xterm's `paste` event and stays.

Tests (`ui.test.ts`): the three outcomes; Ctrl+C without a selection
is `pass`; Ctrl+Shift+C without one is `pass`.

## 4. Ask before killing a detected agent

`layout/store.ts`: nothing. `PaneLayout.tsx`: `onClose` for a pane whose
id is in `agents` sets `confirm = { id, agent }`; a modal — `claude is
running in this pane. Close it?` / Cancel / Close pane — calls
`closePane` on confirm. A pane with no *detected* agent closes at once
as today (the manual Agent label does not count, matching the sidebar
dot). `CloseGuard.tsx`: `onCloseRequested` also gathers the panes with
detected agents; the existing modal lists them under a second heading
(`Agents running: claude in veal`) and its confirm button reads `Quit
anyway`. Rust's kill-on-Destroyed is unchanged.

Tests: none new beyond the pure `liveProjectIds`; acceptance covers the
modals.

## 5. Commands off the main thread

`lib.rs`: every command becomes `#[tauri::command(async)]`, so Tauri
runs it on the async runtime instead of the main thread; the two known
blockers, `pty_write` and `fs_delete`, become `async fn` and wrap their
call in `tauri::async_runtime::spawn_blocking` (the `Arc<PtyRegistry>`
is cloned out of `State` first; both already return `Result`, which
`async fn` with a borrowed `State` requires). The `TasksLock` and `Db`
mutexes still serialise their own callers, which is the intent. Nothing
changes on the JS side.

Acceptance is the measurement: the 2 MB paste again, with
`settings_get_all` in flight — it must answer in **< 50 ms** (was
4735 ms); the 3000-file Recycle-Bin delete with `path_kind` in flight,
same bound (was 1743 ms).

## 6. A fresh page kills what the old page left; no browser menu

`pty.rs`: `kill_all` is already there. `lib.rs`: a `pty_reset` command
calls it. `App.tsx`, `boot`: `await invoke("pty_reset")` before the
restore — a boot that finds panes registered is, by definition, a page
whose predecessor is gone (F5, "Refresh", the dev loop's full reload).
Their `pty-exit` events reach a fan-out with no handlers and are
remembered in `earlyExits`; those entries are cleared in `pty_reset`'s
caller (`pty.ts` exports `resetSessions()`), so no stale exit can land
on a new pane that reuses an id.

`main.tsx`: `document.addEventListener("contextmenu", e =>
e.preventDefault())`. The sidebar's and tree's own menus open from
React handlers that run first and are unaffected; text inputs lose the
native cut/copy/paste menu, which the keyboard still provides.

Tests: `pty.test.ts` — `resetSessions` empties `earlyExits` and
`dataHandlers`. Acceptance: reload with three panes open →
`powershell.exe` children of `veal.exe` are the three new ones only.

## 7. Measure the poller before touching it

`agents.rs`: the debug line already prints a refresh over 50 ms. Lower
the threshold to **10 ms** for this plan's acceptance and record the
p50 / max over 60 s idle with two panes, with no build running. If p50
is under **5 ms** the 3.4 % was the build and nothing changes. If it is
over, `TICK` goes 1.5 s → **3 s** (the badge and dot arrive within 3 s
instead of 1.5 s; plan-08 chose 1.5 s without a cost figure). No
structural change either way.

## 8. Only absolute paths pin

`db.rs`, `projects_add`: after `normalize_path`, refuse a path that is
not absolute (`Path::is_absolute()`; on Windows that is a drive letter or
UNC prefix) with `path must be absolute: {path}`, and one with a `..`
component with `no .. in a project path`. `paths.ts`, `deriveProject`: a
typed root that is relative (no drive, no leading `\\` or `/`) is taken
as **under home**, `joinPath(home, r)`, `created` when it does not exist
— the hint already says "Will be created" and now says where:
`Will be created in ~`. `..` typed anywhere reaches Rust's refusal and
shows as the modal's error.

Tests: `db.rs` — `foo`, `..\x`, `C:\a\..\b` refused; `C:\a` and
`\\srv\share\a` accepted. `paths.test.ts` — `deriveProject("", "foo",
home)` → `~\foo`, `created: true`; an absolute root unchanged.

## 9. One pin per directory; no drive roots

`db.rs`, `projects_add`: refuse a path whose `parent()` is `None`
(`C:\`, `\\srv\share`, `/`) with `pin a folder, not a drive`. Then
compare against every existing pin by identity: `canonicalize` both and
compare; if either fails to canonicalise, fall back to a case-insensitive
string compare on Windows (byte compare elsewhere). A match is `already
pinned: {existing path}` — the existing row's spelling, so the user sees
which one. The `UNIQUE` constraint stays as the last line of defence.

Tests: `C:\` refused; `c:/users/USER/VEAL` after `C:\Users\user\veal`
(a tempdir with mixed case) refused naming the first; a different
directory still pins.

## 10. Case-only rename only when it is the same file

`fs.rs`, `rename`: when `same_entry` and `target.exists()`, also require
`canonicalize(target) == canonicalize(path)` — true on a case-insensitive
filesystem (both resolve to the one on-disk entry), false when a
distinct `A.txt` exists beside `a.txt`. The refusal is the existing
`already exists: {name}`.

Tests: on every platform the existing case-only test still passes; a
new test creates both spellings where the filesystem allows it
(`cfg(unix)`) and asserts the refusal and that neither file changed.

## 11. Flush persistence before the window goes

`layout/persist.ts` and `files/persist.ts`: each exports `flush()` —
cancel the pending timer and write now, returning the promise; a no-op
when nothing is pending. `CloseGuard.tsx`: `onCloseRequested` always
`preventDefault()`s, awaits both flushes (bounded by a **1000 ms**
timeout so a stuck IPC cannot pin the window open), then — no dirty
files and no agents (4) — `destroy()`; otherwise the modal, whose
confirm already destroys. Rust's kill-on-Destroyed is unchanged.

Tests: `layout/persist.test.ts` — a change followed by `flush()` writes
once, immediately, and the debounce timer does not write again;
`flush()` with nothing pending resolves without a write.

## 12. Single instance

`Cargo.toml`: `tauri-plugin-single-instance = "2"`. `lib.rs`: registered
**first**, as its docs require, with a callback that finds the `main`
window, `unminimize()`s, `show()`s and `set_focus()`es it. A second
launch therefore focuses the running window and exits; its argv is
ignored (Veal takes none).

Test: none unit; acceptance launches the exe twice.

## 13. Name the missing shell

`pty.rs`, `spawn`: before `openpty`, a `shell` with no path separator is
looked up on `PATH` the way `default_shell` looks for `pwsh.exe`; not
found → `Err("{shell} is not on PATH")`, so the footer reads
`failed to start shell: pwsh.exe is not on PATH`. A shell given as a
path is checked with `is_file` → `{shell}: not found`. The
`CreateProcessW` string can still appear for a shell that exists and
fails to start, which is the honest message then.

Tests: `spawn` with `shell: Some("zz-no-such-shell.exe")` → exactly that
error, nothing registered.

## 14. Strip a BOM from tasks.json

`tasks.rs`, `parse`: drop a leading `EF BB BF` before `from_slice`. The
file is rewritten without it on the next mutation (the render never
adds one) — the one case where a byte the user did not write is
removed, and it is the one `serde_json` cannot read. Status strictness
stays (decision 5).

Test: a BOM-prefixed file lists its tasks; `add` then writes a file
that does not start with the BOM.

## 15. Show a missing root

`db.rs`, `Project` gains `missing: bool` — `!Path::new(&path).is_dir()`
at `projects_list` time; `db.ts` mirrors it. `projects/store.ts`: `load`
keeps it; a new `markMissing(id, missing)`; `TerminalPane` on a spawn
error that starts with `no such directory` calls `markMissing` for
`projectForCwd(cwd)`; a successful spawn clears it. `Sidebar.tsx`: a
missing row shows its path struck through with the title `Folder not
found — restore it or unpin`, and its `+` is disabled with the same
title. Files and Tasks already error on their own.

Tests: `db.rs` — a project whose directory is removed lists
`missing: true`; `projects/live.test.ts`-style pure test for the store's
flag.

## 16. Tighter snapshot and drag guards

`layout/persist.ts`, `parseNode`: an id must be `Number.isSafeInteger`
and `> 0`. `layout/rects.ts`, `dragWeight`: a non-finite delta returns
the current weight. Both are corrupted-input paths; the fixes are one
line each.

Tests: `parseLayout` with `-5` and `1e300` → `null`; `dragWeight([1,1],
0, NaN, 100)` → `1`.

## 17. Undo a half-spawn; one transaction for a shift

`pty.rs`, `spawn`: if either `thread::Builder::spawn` fails, kill the
child, remove the pane from `panes` and `outboxes`, and return the
error (the reader, if it started, ends at EOF once the master drops).
`db.rs`, `inbox_shift`: the two `UPDATE`s run inside
`conn.unchecked_transaction()` and commit together.

Tests: `inbox_shift` — a forced failure between the updates (a
`CHECK`-less approach: assert positions are unique after every shift in
the existing test, plus one that swaps and reads back inside the same
lock). The spawn path has no injectable failure; it is code review.

## 18. Count UTF-16 units

`fs.rs`, `validate_name`: `name.encode_utf16().count() > 255`.
`files/names.ts`: `name.length > 255` (already UTF-16 units in JS —
confirm, the test tells). `COM0` / `LPT0` stay legal (decision 6).

Tests: 64 fish emoji accepted, 128 refused, 255 `x` accepted, 256
refused — same table in both languages.

## Acceptance (manual, dev window, then the release exe)

1. `mklink /J link target` inside a pinned project → Delete on `link`
   removes the link, `target` untouched; Rename renames the link.
2. Open a Latin-1 file → pill `not UTF-8`, Edit disabled; the bytes on
   disk are unchanged after any interaction. A UTF-8 file edits as
   before.
3. Select text in a pane running `ping -n 30 127.0.0.1`, Ctrl+C → the
   clipboard holds the text, ping keeps running; Ctrl+C with no
   selection stops it.
4. Run `claude` in a pane, click ✕ → modal; Cancel keeps it. Quit the
   window → modal names `claude in veal`; Quit anyway ends it.
5. The two timings in section 5, both under 50 ms.
6. Three panes, F5 (dev) → three `powershell.exe` under `veal.exe`, not
   six. Right-click anywhere → no browser menu; the sidebar and tree
   menus still open.
7. Poller figures recorded in this file's status line; `TICK` changed
   only if the rule in section 7 says so.
8. Add project with root `foo` → pinned as `~\foo`; root `..` → error in
   the modal; nothing appears under `src-tauri`.
9. Root `C:\` → `pin a folder, not a drive`; `c:/users/USER/VEAL` →
   `already pinned: C:\Users\user\veal`.
10. (unix only) `a.txt` → `A.txt` beside an existing `A.txt` refused.
11. Close a pane and hit ✕ at once → the pane is gone on relaunch.
12. Launch the release exe twice → one window, focused.
13. Settings → pwsh on this machine → `failed to start shell: pwsh.exe
    is not on PATH`.
14. `Set-Content -Encoding UTF8` a tasks.json → the board loads.
15. Rename a pinned folder away → the row is struck through, `+`
    disabled; rename it back → normal on the next select.
16–18. `pnpm test`, `cargo test` green with the new cases.
19. `pnpm build`, `pnpm tauri build` green.
20. Commit: `Fix junction ops, lossy reads, Ctrl+C copy, agent close
    guard, async commands, reload orphans, path pins, and eleven more
    pressure-test bugs`.

## Decisions (simplest option each)

1. **Confine the parent for rename/delete; keep following links for
   read/list.** Alt: never follow links anywhere (breaks reading through
   an in-project junction, which pnpm layouts rely on); a `link` kind in
   `Entry` (a feature: the tree does not draw links yet).
2. **Lossy files are read-only.** Alt: remember the source encoding and
   re-encode on save (needs an encoding detector and `encoding_rs`; the
   wrong guess is the same corruption); refuse to open (the user loses
   the read view they have today).
3. **Ctrl+C copies only with a selection.** Alt: Ctrl+Shift+C only
   (Windows Terminal users press Ctrl+C); a copy-on-select option (a
   setting nobody asked for).
4. **Only a detected agent gates a close.** Alt: any child process
   (needs a "has children" query per close and would nag on `cargo
   build`); the manual Agent label (an idle shell labelled Agent would
   nag).
5. **`#[tauri::command(async)]` everywhere, `spawn_blocking` for the two
   known blockers.** Alt: a dedicated thread per PTY writer with a
   channel (more code, same effect); leave reads on the main thread
   (they are cheap, but consistency is simpler to reason about).
6. **Kill leftover PTYs at boot.** Alt: `beforeunload` in JS (an
   `invoke` during unload is not guaranteed to be sent); track the
   webview's page-load event in Rust (Tauri exposes it, but the boot
   call is one line and testable).
7. **Measure the poller; change `TICK` only on evidence.** Alt: raise it
   now (a slower badge for a cost that may be the build's).
8. **Relative typed roots go under home; Rust refuses anything
   relative.** Alt: resolve relative to the exe's cwd (what happens
   today, by accident); refuse in the modal only (Rust is the boundary).
9. **Identity by `canonicalize`, string fallback.** Alt: a normalised
   `key` column with a migration (more moving parts for a few rows);
   refuse only exact-case duplicates (the bug).
10. **Canonical-path equality for case-only renames.** Alt: the
    `same-file` crate (a dependency for one comparison).
11. **Always intercept close, flush with a 1 s bound, then destroy.**
    Alt: write the layout synchronously on every change (a DB write per
    pointermove during a drag — plan-03 added the debounce for that).
12. **The official single-instance plugin.** Alt: a named mutex by hand
    (the plugin is that, plus the focus callback).
13. **Check PATH before spawning.** Alt: parse portable-pty's error
    string (fragile); grey out unavailable shells in Settings (a nicer
    feature, not the fix).
14. **Strip the BOM; keep status strict.** Alt: accept case-insensitive
    or free-form statuses (the file is the contract agents write to;
    plan-05 chose strict so a typo is caught, and the error names the
    task).
15. **`missing` from `projects_list` plus a spawn-failure mark.** Alt:
    poll `is_dir` per project on a timer (the watcher cannot report its
    own root going away on Windows); a modal (the row is enough).
16. **Positive safe integers only.** Alt: clamp (a corrupted snapshot
    is better dropped than repaired).
17. **Undo the maps on thread failure.** Alt: spawn the threads before
    inserting (the waiter needs the entry to exist to remove it).
18. **UTF-16 units, the NTFS limit.** Alt: keep bytes (refuses legal
    names) or code points (allows illegal ones).
