# Veal — plan for step 14: four more bugs from the codebase review

Status: BUILT 2026-09-17 — written and built in one go at the user's
request, right after plan-12. Same rule: the smallest change that
removes each bug.

Scope, findings 6–9 of the 2026-09-17 review:

6. **An exit that beats `pty_spawn`'s reply is lost** (`pty.ts`,
   `TerminalPane.tsx`). `onExit` is registered once the invoke resolves;
   a shell that dies at once (bad `shell` setting, broken profile) can
   emit `pty-exit` first, the fan-out finds no handler and drops it, and
   the pane sits blank with every keystroke logging `no pty N`.
7. **The layout degrades to one stack after a close** (`layout/tree.ts`).
   `remove` collapses `row(col(1,3), col(2,4))` minus 1 and 3 to
   `col(2,4)`; `insert` then only ever splits into columns, so the next
   two panes give `col(col(2,a), col(4,b))` — four panes stacked.
8. **The popover's own trigger reopens it** (`TypePopover.tsx`,
   `NewTerminalButton.tsx`, `Sidebar.tsx`). The popover closes on any
   document `pointerdown` outside it, then the trigger's `click` toggles
   `false → true`. Only Escape or a click elsewhere dismisses it.
9. **Saving strips the hidden/system attribute; a read-only file gives
   `rename …: Access is denied`** (`fs.rs`). Measured on Windows before
   planning: rename-over succeeds on hidden and system files but the new
   file has neither bit; on a read-only file it fails, and so would an
   in-place write, so refusing is right — the message is not. The review
   also said symlinks are flattened: they are not, `confine` canonicalises
   the path first, so the write already reaches the target.

## Dependencies

None.

## 6. Remember an early exit

`pty.ts`: the `pty-exit` fan-out, finding no handler for an id, records
`earlyExits.set(id, code)`. `onExit(cb)` for an id in `earlyExits` calls
`cb` on a microtask and clears the entry, so the pane's existing
`.then` path (`onExit` → footer or close) runs unchanged. The same for
`pty-agent`: not needed — detection polls every 1.5 s, long after spawn
resolves — and not done.

Test (`pty.test.ts`, new; `@tauri-apps/api` mocked): an exit event
delivered before `onExit` is registered still reaches the callback, once;
a normal exit is unaffected; an early exit for one id does not leak to
another.

## 7. Keep the root a row

`layout/tree.ts`, `insert`: a root split that is not a `row` is wrapped
first — `split("row", [root, p])` — so `col(2,4)` + a becomes
`row(col(2,4), a)`, and the next insert gives `row(col(2,4), col(a,b))`:
the grid the comment promises. No other path can produce a `col` root.

Test: close 1 and 3 from the four-pane grid, add two → `["row",
["col", 2, 4], ["col", 5, 6]]`.

## 8. The trigger toggles

`TypePopover.tsx`: the outside-`pointerdown` closer ignores a target
inside an element marked `data-popover-anchor`. `NewTerminalButton` puts
the attribute on its button and toggles as it does; the Sidebar's `+`
gets it and toggles too (`spawnFor === id ? null : id`). Everything else
— Escape, a click anywhere else, picking a kind — is unchanged.

## 9. Attributes survive a save; read-only is named

`fs.rs`, `write`: the target's metadata is read up front (it is already,
when `expected_mtime` is given). A read-only target → `read-only on
disk` before anything is written, no temp file. After the rename, on
Windows, the hidden / system / archive bits the old file had are put
back with `SetFileAttributesW` (`windows-sys`, already in the tree via
Tauri — only the `Win32_Storage_FileSystem` feature is new). Elsewhere
nothing changes: unix has no such bits and the mode is already kept by
the rename semantics we accept.

`files/store.ts`, `save`: the new message is not a conflict, so it
surfaces as the existing `Save failed: …` toast — no UI change.

Tests: `write_refuses_a_read_only_file` (every platform: `set_readonly`),
`write_keeps_the_hidden_attribute` (`cfg(windows)`, `attrib +h` via
`SetFileAttributesW`, assert the bit after a save).

## Acceptance (manual, dev window)

1. Settings → shell → a program that exits at once (`cmd.exe` with a
   `/c exit 3` wrapper, or point `shell` at `where.exe`) → a new pane
   shows `process exited (code 3)` instead of a blank terminal.
2. Four panes, close the two on the left, add two → a 2×2 grid.
3. Open "+ New terminal", click it again → closed. Same for a project's
   `+` in the sidebar.
4. `attrib +h` a file, save it from Veal → still hidden. `attrib +r` a
   file → Save gives `Save failed: read-only on disk`, the file is
   untouched and no `.veal-tmp` is left behind.
5. `pnpm test`, `cargo test`, `pnpm build` green.
6. Commit: `Fix early PTY exit, column-root layout, popover toggle, save attributes`.

## Decisions (simplest option each)

1. **Buffer early exits by id in the fan-out.** Alt: register the
   handler before the invoke (the id is not known yet); return the exit
   in the spawn reply (Rust would have to wait, and every spawn would
   pay for the rare case).
2. **Wrap a non-row root on insert.** Alt: keep the root a row in
   `remove` (a split needs two children, so a one-column row is not
   representable); a smarter placement rule (plan-02 chose this one on
   purpose).
3. **An anchor attribute the closer ignores.** Alt: `stopPropagation` on
   the trigger's pointerdown (also hides it from the context-menu
   closers); a ref per trigger (the sidebar has one per project).
4. **Restore the attributes after the rename; refuse read-only.** Alt:
   write in place (Windows refuses to open a hidden file for truncation,
   and a crash mid-write tears the file); clear the read-only bit and
   write anyway (the bit is there for a reason; an editor that wants
   this offers it as a second step, which nobody has asked for).
