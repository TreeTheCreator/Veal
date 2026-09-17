# Veal — plan for step 7: editing in the Files view

Status: APPROVED 2026-09-17 (decision 2 amended: explicit Edit toggle; the
rest as recommended).

Scope: the read-only CodeMirror viewer becomes an editor. Any text file the
viewer already shows can be edited after an explicit **Edit** toggle in the
head — files open read-only so a stray keystroke cannot change them. Once
edited the toggle becomes **Save**; `Ctrl+S` or Save writes the file back
atomically, with line endings and BOM preserved, and returns it to
read-only. A save is refused if the file changed on disk since it was read — the same "never clobber an agent's edit" rule as
`tasks.rs` — and the viewer then offers Overwrite or Reload. Unsaved edits
survive switching files and projects, show as a dot in the tree, and stop
the window closing silently. Nothing else moves: no watcher (step 8, see
decision 1), no file creation / rename / delete, no changes to the Tasks
view, the tree model, `pty.rs` or the schema.

## Dependencies

None new. `history` and `indentWithTab` are in `@codemirror/commands`,
`bracketMatching` / `indentOnInput` in `@codemirror/language` — both already
installed. The close guard uses `@tauri-apps/api/window`, part of the API
package we have. Rust: `fs_write` is `std::fs`.

Considered and rejected:
- **`notify` (file watching) in this step.** The mtime check below already
  makes clobbering impossible; a watcher only makes the tree, the open file
  and the Tasks board *fresh* without a Refresh click. That is its own step
  (Files tree + viewer + Tasks store + debounce), not a rider on this one.
- **`@codemirror/autocomplete`** for `closeBrackets` — a new package for a
  feature that half the time fights the typist. Not in step 7.
- **Autosave** (on blur / debounce). Agents and `git` operate on these files;
  a half-typed line landing on disk because focus moved to a pane is worse
  than an unsaved dot. Manual `Ctrl+S` only (decision 4).
- **Editable on open** (no toggle). A dev environment where every file is
  live under the cursor invites accidents — a stray paste into `Cargo.lock`
  while reading it. Files open read-only; editing is opt-in per file
  (decision 2).
- **Tabs** for open files. The tree is the tab strip: the open file is
  highlighted, dirty files carry a dot, and drafts survive switching.
- **Writing in place** instead of tmp + rename. In-place keeps the file's
  identity (ACLs, hard links, a process holding it open) but a crash
  mid-write truncates it. Same choice as `tasks.rs` (decision 7).

## Rust (`src-tauri/src/fs.rs`)

`FileText` gains what a save needs to give back:

```rust
struct FileText { text: String, size: u64, mtime: u64 /* ms since epoch */, bom: bool }
struct Written  { size: u64, mtime: u64 }
```

`read` still strips the BOM and keeps line endings as-is; `bom` says
whether one was stripped. `mtime` is `metadata().modified()` in whole
milliseconds (fits a JSON number exactly; NTFS stamps are 100 ns so two
external writes inside one millisecond are the only blind spot).

New `write(path, text, crlf, bom, expected_mtime: Option<u64>) -> Result<Written, String>`:

- **Conflict check first.** `expected_mtime: Some(m)` → the file's current
  mtime must equal `m`, else `Err("changed on disk")`; a missing file →
  `Err("deleted on disk")`. `None` skips the check (Overwrite).
- CodeMirror joins lines with `\n`; if `crlf`, every `\n` becomes `\r\n`.
  If `bom`, `\u{feff}` is prepended. Nothing else is touched — no trailing
  newline is added or removed.
- Atomic: `<name>.veal-tmp` beside the file, then `rename` over it. The
  returned `mtime` is read back from the new file so the next save's check
  matches what is on disk.
- No size cap on write; the read cap (2 MiB) already bounds what can be
  loaded into the editor.

Command `fs_write(db, path, text, crlf, bom, expected_mtime)` in `lib.rs`,
confined via `fs::confine` like the others.

**Window close:** `on_window_event` currently kills every PTY on
`CloseRequested`. Once JS listens for close (decision 9) Tauri prevents the
close and lets JS decide, so a cancelled close would leave dead panes.
`kill_all` moves to `WindowEvent::Destroyed`.

## Frontend

### `src/fs.ts`

`FileText` gets `mtime` and `bom`; `fs.write(path, text, opts: { crlf, bom, expectedMtime: number | null })` → `Written`.

### `src/files/store.ts`

`FileState.loaded` grows, and drafts join `ProjectFiles`:

```ts
type FileState =
  | { status: "loading" }
  | { status: "loaded"; text: string; size: number; mtime: number; bom: boolean; crlf: boolean;
      saving: boolean; conflict: "changed" | "deleted" | null }
  | { status: "error"; error: string };

interface Draft { state: EditorState | null; mtime: number }   // exists only while dirty
interface ProjectFiles { …; editing: Set<string /* pathKey */>; drafts: Map<string, Draft> }
```

- `editing` holds the files whose Edit toggle is on. `setEditing(projectId,
  path, on)` adds/removes; turning it off is only possible while clean
  (the button is Save by then). A file can be dirty only while editing.
  Save and Discard both clear `editing`, so a saved file is read-only again
  until Edit is clicked. `editing` survives switching files and projects
  like drafts do; `forget` drops it.

- `text` is the **text as loaded**, never the live document. It changes only
  when a read replaces the file, so the editor's "new document" effect keys
  on it and a save does not reset the cursor.
- `crlf` is `text.includes("\r\n")` at read time.
- A file is **dirty** iff `drafts` has its key. `markDirty(projectId, path)`
  creates the entry with the loaded `mtime` (one store write on the first
  keystroke, none after). `stash(projectId, path, state)` stores the live
  `EditorState` when the editor moves to another file, so undo history and
  selection come back with the draft. Clean files are not stashed — their
  undo history is lost on switch (decision 6).
- `read` (open, Refresh, project switch) with a dirty draft present:
  same `mtime` → keep the draft, no state change; different `mtime` → set
  `conflict: "changed"` and **keep** the loaded `text`/`mtime` so the
  editor is not reset. Without a draft, a read replaces everything as
  today. A read error on a dirty file (deleted) → `conflict: "deleted"`.
- `save(projectId, path, text, force = false)`: `saving: true` →
  `fs.write(path, text, { crlf, bom, expectedMtime: force ? null : mtime })`
  → on success update `size`/`mtime`, clear `saving` and `conflict`, drop
  the draft and clear `editing`; on `changed on disk` / `deleted on disk`
  set `conflict`;
  any other error surfaces as a toast (permission, disk full) and the draft
  stays.
- `discard(projectId, path)`: drop the draft, clear `editing`, re-read.
- `dirtyPaths(): Array<{ project, path }>` across projects, for the close
  guard and the tree dots. `forget` drops drafts with the project.

### `CodeViewer` / `Editor` (`src/components/CodeViewer.tsx`)

- Extensions: `readOnly` / `editable` move into a second **Compartment**
  reconfigured when the Edit toggle flips, so toggling keeps scroll,
  selection and history rather than rebuilding the state. The `tabindex`
  hack stays for read-only mode (`Ctrl+F` still needs focus). Added:
  `history()`, `historyKeymap`, `indentWithTab`, `indentOnInput()`,
  `bracketMatching()`, `highlightActiveLine()` (theme: `--panel` at 40 %,
  editing only), and an `updateListener` that calls `markDirty` on the
  first `docChanged` while the file is clean. `Mod-s` in the keymap saves
  when dirty. Language chunk and its Compartment as today.
- The new-document effect keys on `projectId:pathKey` + `text`: it uses the
  stashed `EditorState` when a draft has one, else creates a fresh state.
  Before switching away it stashes the live state if the file is dirty.
- Head: `<project> / <b>rel</b>` · grow · size · the **toggle button** ·
  pill. The one button reads, in order: **Edit** (read-only → editing),
  **Done** (editing, clean → read-only), **Save** (dirty → write, then
  read-only). While dirty a ghost **Discard** button sits beside Save
  (`discard`, then read-only) — the only way to abandon edits short of the
  close guard. Pill: `read-only` as today when not editing, nothing when
  editing and clean, `modified` (`--accent`) when dirty, `saving…`,
  `conflict` (`--agent`). Binary / too-large files have no button.
- Conflict banner between head and editor (`.viewer-banner`):
  `Changed on disk since you opened it.` / `Deleted on disk.` with
  **Overwrite** (save with `force`) and **Reload** (`discard`). The editor
  stays usable underneath, still in edit mode.
- Window-level `Ctrl+S` while the Files view is showing: `preventDefault`
  always (WebView2 otherwise opens its own "Save page" dialog) and save the
  open file if dirty, even when focus is in the tree; it does not enter edit
  mode. Same shape as the
  `Ctrl+F` handler. `Ctrl+1/2/3` are unchanged.
- Empty-viewer hint becomes `Edit to change a file · Ctrl+F searches`.

### `FileTree`

A dirty file's row gets a `●` (`--accent`, 8 px) after the name; the row
also gets `title="unsaved changes"`. Rows know dirtiness from a `dirty:
Set<pathKey>` prop built from `drafts`.

### Close guard (`src/components/CloseGuard.tsx`, mounted in `App`)

`getCurrentWindow().onCloseRequested(e => …)`: if `dirtyPaths()` is empty,
let it through; else `e.preventDefault()` and open a modal listing the
files (`veal / src/App.tsx`, …) with **Discard and quit** (drops drafts,
`getCurrentWindow().destroy()`) and **Cancel**. No "Save all": other files'
live text lives in stashed `EditorState`s, and saving several files with
conflict handling from a modal is more UI than the case deserves —
`Ctrl+S` them first. `core:default` covers the event listener; if `destroy`
turns out to need it, `core:window:allow-destroy` joins the capability.

### `styles.css`

`.viewer-banner` (agent-soft background, agent border-bottom, 34 px, mono
12 px, two `.btn`s), `.node .dirty` dot, `.cm-activeLine` colour, and the
save button reuses `.btn`.

## Tests

- Rust `cargo test` (`fs.rs`, tempdir):
  - `read` reports `bom: true` and the stripped text for a BOM file; `mtime`
    matches `metadata().modified()` in ms.
  - `write` round-trips `one\ntwo\n` with `crlf: true` → bytes
    `one\r\ntwo\r\n`; with `bom: true` → leading `EF BB BF`; `read` after
    `write` gives back the same `text`, `bom`, and the returned `mtime`.
  - `write` with `expected_mtime` = the current stamp succeeds; with a stale
    stamp → `changed on disk` and the file's bytes are untouched; on a
    missing file with `Some` → `deleted on disk`, with `None` → created.
  - No `.veal-tmp` remains after a write.
- vitest `files/store.test.ts` (mocked `fs`):
  - `markDirty` then `read` with the same `mtime` keeps the draft and the
    text; with a newer `mtime` sets `conflict: "changed"` and keeps the text.
  - `save` calls `write` with `expectedMtime` = loaded `mtime`, `crlf` from
    the text, `bom` from the read; success updates `mtime`/`size` and drops
    the draft; a `changed on disk` rejection sets `conflict` and keeps it;
    `force` passes `expectedMtime: null`.
  - `discard` drops the draft, clears `editing`, and re-reads; `save`
    success clears `editing`; `setEditing(off)` is ignored while dirty;
    `forget` drops drafts and `editing`; `dirtyPaths` spans projects.
- vitest `files/eol.test.ts`: `detectCrlf` — `"a\r\nb"` → true, `"a\nb"` →
  false, `""` → false.

## Acceptance (manual, dev window)

1. Open `src/App.tsx`, type → nothing happens, pill `read-only`. Click
   **Edit** → caret blinks, button reads **Done**; click Done → read-only
   again, scroll position kept. Edit, type a character → tree row shows
   `●`, pill `modified`, button reads **Save**, ghost **Discard** beside
   it. `Ctrl+S` → pill back to `read-only`, button **Edit**, `cat` in a
   pane shows the change, CRLF/LF unchanged (`Format-Hex | head`).
   Edit, type, **Discard** → text reverts, read-only.
2. Open `pnpm-lock.yaml` (LF), `.gitignore` (whatever it is) and a CRLF
   file (create one with `Set-Content`): edit + save each → `git diff` shows
   only the edited line, no whole-file EOL churn.
3. A file with a BOM (`Set-Content -Encoding utf8BOM`) keeps its BOM after
   save; `Format-Hex` starts `EF BB BF`.
4. Edit `App.tsx`, switch to `db.rs`, edit that, switch back → both drafts
   intact, still in edit mode, with cursor/undo (`Ctrl+Z` works); both rows
   dotted. Switch projects and back → still there. A file left in edit mode
   but clean is still in edit mode when you return.
5. Edit `App.tsx` without saving; in a pane `Add-Content src\App.tsx "// x"`
   → Refresh → pill `conflict`, banner, editor still has the draft.
   **Reload** → draft gone, `// x` visible. Repeat, choose **Overwrite** →
   the file has the draft, not `// x`.
6. Edit a file, `rm` it in a pane, `Ctrl+S` → `Deleted on disk.` banner;
   **Overwrite** recreates it.
7. `Ctrl+S` with focus in the tree saves the open file and no WebView2
   "Save page" dialog appears; `Ctrl+S` on a clean or read-only file does
   nothing (and does not enter edit mode).
8. Tab inside the editor indents; `Esc` then `Tab` leaves the editor.
9. Close the window with two dirty files → modal lists both; Cancel keeps
   the app and the panes alive (type in one); Discard and quit exits, and
   `veal.exe` is gone from `tasklist` (PTYs killed on Destroyed).
10. Edit `.veal/tasks.json` in the editor (change a status), save, `Ctrl+3`
    → the board shows it.
11. Binary and >2 MB files still show their messages, no Save button.
12. `pnpm test`, `cargo test`, `pnpm build` green; bundle grows by little
    (history/indent are small).
13. Commit: `Add editing to the Files view: Edit toggle, Ctrl+S, drafts, conflict detection`.

## Findings during acceptance (2026-09-17)

- **Saving a file of the running app itself bounced the view to Terminal.**
  Not the editor: saving `src/*.tsx` under `pnpm tauri dev` triggers Vite's
  reload, and saving `src-tauri/src/*.rs` makes cargo rebuild and restart
  the process — and the app always opened on Terminal (plan-03). The view
  is now written to `settings` (`view` key) on every switch and restored at
  boot, so reloads, restarts and ordinary launches all come back to the
  view you were on. This supersedes plan-03's "always opens on Terminal";
  the tree and open file are still in memory only (plan-04, decision 7), so
  after a full restart the Files view shows a collapsed tree.
- `destroy` is not in `core:window:default`; `core:window:allow-destroy`
  was added to the capability for the close guard.
- The `Editor` component unmounts during a file's loading state, so the
  CodeMirror compartments are module-level (one view at a time) — a stashed
  draft from an earlier mount could not otherwise be toggled back to
  read-only — and the window-level `Ctrl+S` / `Ctrl+F` handler lives on
  `CodeViewer`, which stays mounted, so `Ctrl+S` is swallowed even with no
  file open.
- Switching back to a dirty file that was deleted meanwhile synthesises a
  loaded state from the stashed draft with `conflict: "deleted"`, so
  Overwrite can recreate it.

## Decisions to confirm

1. **No file watcher in this step.** Correctness comes from the mtime check;
   freshness (tree, open file, Tasks board) is step 8 with `notify`.
   Alternative: fold the watcher in now.
2. **Files open read-only; an Edit toggle in the head turns editing on per
   file.** The same button becomes Done (clean) or Save (dirty); Save and
   Discard return the file to read-only. *Amended from the draft's
   "editable on open" — no accidental edits.*
3. **Conflict detection by mtime (ms) with an `expected_mtime` on every
   save**; a mismatch refuses the write and offers Overwrite / Reload.
   Alternative: content hash (a re-read per save), or no check.
4. **Manual save only (`Ctrl+S` / Save button).** No autosave.
5. **Line endings and BOM are preserved per file** (`crlf` detected at read,
   `bom` reported by Rust) rather than normalised to LF. Alternative:
   always write LF, no BOM.
6. **Drafts survive file and project switches; undo history is kept only
   for dirty files.** Clean files get a fresh state on return. Alternative:
   stash every visited file's state (memory grows with files visited).
7. **Atomic write: tmp + rename**, matching `tasks.rs`. Alternative: in
   place, keeping file identity.
8. **Conflict banner keeps the editor live** — you can keep typing and
   decide later. Alternative: lock the editor until resolved.
9. **Close guard with a modal: Discard and quit / Cancel, no Save all.**
   Requires moving `kill_all` to `WindowEvent::Destroyed`. Alternative: no
   guard, unsaved edits die with the window.
10. **Editor extras: history, indentWithTab, indentOnInput, bracketMatching,
    highlightActiveLine. Not: closeBrackets, autocomplete, line wrapping,
    format-on-save.** Alternative: `basicSetup` (pulls autocomplete etc.).
11. **Dirty indicator: a dot after the tree row name + a `modified` pill**;
    the Files tab in the top bar is unchanged. Alternative: also dot the tab.
12. **No create / rename / delete of files** — the shell does that; Refresh
    picks it up. Later, with the watcher.
