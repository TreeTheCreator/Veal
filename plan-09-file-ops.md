# Veal — plan for step 10: create, rename and delete in the Files tree

Status: BUILT 2026-09-17 — written and built in one go at the user's
request; the decisions at the end are the ones the build follows. Flag any
to flip and the change is made before commit.

Scope: the Files tree gains the four operations plan-06 (decision 12) left
to the shell: **New file**, **New folder**, **Rename**, **Delete**. Names
are typed inline in the tree (an editable row, VS Code style), not in a
modal. Delete moves to the **Recycle Bin** (`trash` crate), so it needs no
confirmation unless the file — or a file under the folder — has unsaved
edits. Every operation is a Rust command confined to pinned roots, and the
tree refreshes both from the explicit re-list after the call and from the
watcher (plan-07). Nothing else moves: no move/drag between folders, no
copy/duplicate, no multi-select, no undo beyond the Recycle Bin, no change
to the Tasks view or the schema.

## Dependencies

- **`trash = "5"`** (Rust). `IFileOperation` on Windows (the real Recycle
  Bin, same as Explorer), `NSFileManager` on macOS, the FreeDesktop trash
  on Linux. One call, `trash::delete(path)`, files and directories alike.

Nothing new on the JS side.

Considered and rejected:
- **Permanent delete with a confirmation modal.** A modal is the cost of
  irreversibility; the Recycle Bin removes the irreversibility instead,
  and a toast (`Moved App.tsx to the Recycle Bin`) is all the feedback a
  recoverable action needs. The one remaining modal is for unsaved edits
  (decision 4), which the bin cannot recover.
- **Modal name prompts.** An inline row keeps the eye where the file will
  appear and needs no positioning; the Sidebar's add-project modal is a
  form with two fields, this is one name.
- **Drag-to-move / copy.** Different interaction model (drop targets,
  auto-expand on hover); a later step if wanted.
- **`tauri-plugin-fs`** — same reasoning as plan-04: three more commands
  in our own `fs.rs`, confined the same way.

## Rust (`src-tauri/src/fs.rs`)

```rust
pub fn validate_name(name: &str) -> Result<(), String>
pub fn create(dir: &Path, name: &str, kind: &str /* "file" | "dir" */) -> Result<PathBuf, String>
pub fn rename(path: &Path, name: &str) -> Result<PathBuf, String>
pub fn delete(path: &Path) -> Result<(), String>
```

- `validate_name`: non-empty after trim, no `\` or `/`, not `.` or `..`,
  no `< > : " | ? *` or control characters, no trailing `.` or space,
  not a Windows reserved device name (`CON PRN AUX NUL COM1–9 LPT1–9`,
  with or without an extension), at most 255 bytes. Checked on every
  platform — a name that is illegal on Windows is a name a teammate on
  Windows cannot check out.
- `create`: `validate_name`, then `File::create_new` / `create_dir` in
  `dir`; an existing entry is `already exists: <name>`. Returns the new
  path.
- `rename`: `validate_name`, then `std::fs::rename` to the sibling path.
  A target that exists is `already exists: <name>` — except a **case-only
  rename** (`readme.md` → `README.md`), which on a case-insensitive
  filesystem *is* the same file and goes through. Returns the new path.
- `delete`: `trash::delete`. A missing path is `not found`.

Commands `fs_create(dir, name, kind)`, `fs_rename(path, name)`,
`fs_delete(path)` in `lib.rs`, confined via `fs::confine` (the parent for
`create`, the path itself for the others; the new name never contains a
separator, so the result is inside the same root by construction).

## Frontend

### `src/fs.ts`

`fs.create(dir, name, kind)`, `fs.rename(path, name)`, `fs.delete(path)`
→ `Promise<string>` (new path) / `Promise<void>`.

### `src/files/names.ts` (pure, vitest)

`validateName(name): string | null` — the same rules as Rust, for
inline feedback as you type. Rust still decides.

### `src/files/rekey.ts` (pure, vitest)

`rekeyProject(p: ProjectFiles, from: string, to: string): ProjectFiles` —
after a rename, every per-project map keyed by the old path or anything
under it (`dirs`, `expanded`, `editing`, `drafts`, `open`) is re-keyed
to the new path; `drafts[*].path` follows. After a delete,
`dropUnder(p, path)` removes those entries and closes the open file if it
was inside.

### `src/files/store.ts`

```ts
/** Inline editing in the tree; one at a time. */
edit: { mode: "create"; projectId; dir: string; kind: EntryKind } | { mode: "rename"; projectId; path: string } | null;
startCreate(projectId, dir, kind); startRename(projectId, path); cancelEdit();
create(projectId, dir, name, kind): Promise<boolean>;
rename(projectId, path, name): Promise<boolean>;
remove(projectId, path): Promise<boolean>;
dirtyUnder(projectId, path): DirtyFile[];
```

- `startCreate` expands `dir` if it is collapsed (so the new row has a
  place to appear) and sets `edit`. `startRename` sets `edit`. Both
  replace any edit in progress.
- `create`: `fs.create` → clear `edit`, `list(dir)`, and for a file
  `openFile` it (a new empty file opens in the viewer; Edit is one click
  away — decision 6). Errors → toast, `edit` stays so the name can be
  fixed.
- `rename`: `fs.rename` → clear `edit`, `rekeyProject`, `list(parent)`.
  Same name → just cancel. Errors → toast, `edit` stays.
- `remove`: `fs.delete` → `dropUnder`, `list(parent)`, toast `Moved
  <name> to the Recycle Bin`. Errors → toast.
- `dirtyUnder` feeds the unsaved-edits modal.

### `FileTree`

- Props gain `edit`, `onCommit(name)`, `onCancelEdit`, `onContextMenu(row
  | null, x, y)`, `onRename(row)`, `onDelete(row)`.
- **Create row**: rendered right after the target directory's row — at
  the top of its children, depth + 1, so it is visible without scrolling
  — or at the top of the tree for the root. Folder/file icon per `kind`, an `<input>` with the tree's
  mono font, `autoFocus`, `select()` on mount.
- **Rename row**: the row's name becomes the same `<input>` with the old
  name; the stem (not the extension) is selected on mount.
- `Enter` commits, `Escape` cancels, **blur cancels** (decision 5). While
  the value is invalid the input gets `aria-invalid` and a red outline;
  Enter does nothing and the validation message shows as the row's
  `title`.
- Right-click on a row opens a `ctx-menu` (the Sidebar's class):
  directories — `New file`, `New folder`, `Rename`, `Delete`; files —
  `New file`, `New folder` (in the file's directory), `Rename`, `Delete`.
  Right-click on the tree's empty space — `New file`, `New folder` at
  the root.
- Keys on a focused row: `F2` rename, `Delete` delete. (`Enter`/arrows
  as before.)
- `.node` rows get `draggable={false}` so the browser does not start a
  ghost-drag on the icon.

### `FilesView`

- Toolbar gains `＋ File` and `＋ Folder` buttons after Refresh; they
  create in the **focused row's directory** (a directory row → inside
  it; a file row → its parent; nothing focused → the root). The tree
  reports its cursor row via `onCursor(row | null)`.
- Owns the context menu state (like the Sidebar) and the unsaved-edits
  modal: `Delete <name>? It has unsaved changes.` (or `N files under it
  have unsaved changes`) with **Delete anyway** / **Cancel**.
- Hint becomes `Right-click a row · F2 renames · Del moves to the Recycle
  Bin`.

### `styles.css`

`.node input.rename` (inherits the row font, `--panel-2` background,
`--line-strong` border, `aria-invalid` → `--agent` outline), `.node.ghost`
for the create row, the toolbar's two `.btn`s reuse existing styles.

## Tests

- Rust `cargo test` (`fs.rs`, tempdir):
  - `validate_name`: `a.txt` ok; `""`, ` `, `.`, `..`, `a/b`, `a\b`,
    `a:b`, `con`, `CON.txt`, `lpt1`, `a.`, `a `, 256 bytes → the named
    error; `.gitignore` and `émoji 🐟.md` ok.
  - `create` file then dir; `create` again → `already exists`; the file
    is zero bytes; `list` shows both.
  - `rename` moves the file, returns the new path; onto an existing
    sibling → `already exists`, both untouched; case-only rename succeeds
    and `list` shows the new case; a directory rename keeps its contents.
  - `delete`: **not unit-tested** — it would put tempdir files in the
    developer's Recycle Bin on every `cargo test`; covered by acceptance.
- vitest `files/names.test.ts`: the same table as Rust.
- vitest `files/rekey.test.ts`: renaming `src` → `lib` re-keys `dirs`
  (`src`, `src\files`), `expanded`, `editing`, `drafts` (and each
  draft's `path`) and `open`; renaming a file re-keys only that file;
  `dropUnder` removes them and nulls `open`/`file` when inside; an
  unrelated project slice is untouched.
- vitest `files/store.test.ts`: `startCreate` on a collapsed dir expands
  it; `create` calls `fs.create`, clears `edit`, re-lists the dir and
  opens the new file; a rejected create keeps `edit` and toasts;
  `rename` re-keys the open file and re-lists the parent; `remove` of the
  open file closes it and re-lists; `dirtyUnder` finds a draft below a
  directory.

## Acceptance (manual, dev window)

1. `＋ File` with nothing focused → an input row at the top of the tree;
   type `notes.md`, Enter → the row appears sorted, the file opens
   (empty, read-only); a pane's `ls` shows it. `＋ Folder` on a focused
   `src` → input under `src`; `util`, Enter → folder row.
2. Right-click `src\App.tsx` → `New file` → input inside `src`; Escape →
   gone, nothing created. Click elsewhere (blur) → same.
3. F2 on `notes.md` → input with `notes` selected; type `NOTES`, Enter →
   `NOTES.md` (case-only rename works on NTFS). F2, type `App.tsx`, Enter
   → toast `already exists: App.tsx`, input stays. Type `con`, Enter →
   nothing happens, red outline, title explains.
4. Open `src\App.tsx`, Edit, type; F2 → rename to `App2.tsx` → the viewer
   head shows `src/App2.tsx`, the draft is still there and dirty, `Ctrl+S`
   writes `App2.tsx`; rename back.
5. Rename `src` → `source` while `src\App.tsx` is open with a draft → the
   tree re-keys (still expanded), the open file is `source\App.tsx`, the
   draft survives; rename back.
6. Del on `NOTES.md` (clean) → gone from the tree, toast `Moved NOTES.md to
   the Recycle Bin`; Explorer's Recycle Bin has it. Restore it from the
   bin → the watcher brings the row back.
7. Edit `notes.md`, type, Del → modal `It has unsaved changes.`; Cancel
   keeps everything; Del again, **Delete anyway** → gone, draft dropped,
   viewer empty.
8. Del on the `util` folder → gone (to the bin). Create `util\x.txt`, edit
   it, Del on `util` → modal `1 file under it has unsaved changes`.
9. Every operation on a pinned network/`subst` root works through
   `confine`; a path outside (typed via CDP) is refused.
10. `pnpm test`, `cargo test`, `pnpm build` green.
11. Commit: `Add create, rename and delete to the Files tree: inline names, Recycle Bin`.

## Findings during acceptance (2026-09-17)

- Acceptance 1–9 ran against the dev window over CDP with **real input
  events** (`Input.dispatchMouseEvent` / `dispatchKeyEvent` /
  `insertText`), so the context menu, the inline input, F2, Del, Enter,
  Escape and blur were exercised as a user would: 32 checks, all passing,
  three runs.
- **"Nothing focused" was wrong on the first run.** The tree's cursor
  defaults to row 0, so `＋ File` with nothing focused created the file in
  `dist\` (the first row). The tree now reports its cursor row only while
  focus is inside it, and the toolbar's ＋ buttons prevent `mousedown`
  focus so a focused row stays the target. Unfocused → root, as planned.
- **The context menu ran off the bottom of the window** for a row near
  the edge, so its Delete item was unclickable; `clampMenu` keeps it in
  the viewport (the Sidebar's menu has the same latent issue; its rows are
  near the top).
- The inline input cannot live inside the row's `<button>` (invalid HTML,
  focus fights); a renaming row renders as a `<div>` with the same class.
- A failed commit (`already exists`, from Rust) keeps the input open with
  the typed name; a name that fails `validateName` never reaches Rust.
- `nul.d.ts.bak` is a reserved name (classic Windows rule: the stem before
  the first dot); the Rust test table was corrected, not the rule.
- Rust returns the JS-facing path built from the path JS sent, not the
  confined one — `confine` canonicalises to `\\?\C:\…`, which `pathKey`
  would not match.
- `fs::delete` is exercised by acceptance only (decision 10); the run
  leaves a few `n.ts` entries in the developer's Recycle Bin.
- Not run: renaming `src` itself (it restarts Vite); a sibling directory
  rename covers the re-key path, and `rekey.test.ts` covers the nested
  case.

## Decisions (built as recommended)

1. **Delete goes to the Recycle Bin (`trash`), no confirmation for clean
   files and folders.** Alternative: permanent delete behind a modal.
2. **Inline name editing in the tree**, VS Code style, one edit at a
   time. Alternative: a modal prompt.
3. **Name rules are Windows-strict on every platform** (reserved names,
   `< > : " | ? *`, trailing dot/space). Alternative: only what the host
   OS rejects.
4. **Unsaved edits are the one thing that asks**: deleting a dirty file,
   or a folder with dirty files under it, shows a modal with `Delete
   anyway` / `Cancel`. Alternative: drop the draft silently (it is in the
   bin, but the draft is not).
5. **Blur cancels an inline edit** (Enter commits). Alternative: blur
   commits, like VS Code.
6. **A new file opens in the viewer read-only** (Edit is the next click).
   Alternative: open straight into edit mode.
7. **Case-only renames are allowed** (`readme.md` → `README.md`).
   Alternative: treat as `already exists`.
8. **No move, copy, duplicate or multi-select.** Alternative: drag-to-move
   now.
9. **The toolbar's `＋ File` / `＋ Folder` target the focused row's
   directory**, root when nothing is focused. Alternative: always the
   root.
10. **`delete` has no Rust unit test** (it would fill the developer's
    Recycle Bin); acceptance covers it. Alternative: a test that deletes
    and then empties the bin entry by id.
