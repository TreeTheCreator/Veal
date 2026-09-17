# Veal — plan for step 5: Files view

Status: APPROVED 2026-09-17 (all ten decisions as recommended).

Scope: the Files placeholder becomes a real view. A lazily-loaded directory
tree scoped to the active project's root on the left; a read-only, syntax-
highlighted CodeMirror 6 viewer on the right. Switching the active project
swaps the tree. Nothing else moves: no changes to the layout store, `pty.rs`,
the sidebar or the schema. Tasks (step 6) stays a placeholder. Editing is a
later step, as the spec's open decision allows ("read-only to start").

## Dependencies

| Side | Package                     | Version  | Purpose                                                            |
|------|-----------------------------|----------|--------------------------------------------------------------------|
| JS   | @codemirror/view            | 6.43.12  | `EditorView`, line numbers, `drawSelection`                        |
| JS   | @codemirror/state           | 6.7.5    | `EditorState`, `readOnly`                                          |
| JS   | @codemirror/language        | 6.12.4   | `syntaxHighlighting`, `LanguageDescription`                        |
| JS   | @codemirror/language-data   | 6.5.2    | `languages[]` — one lazy chunk per language, matched by filename   |
| JS   | @codemirror/search          | 6.7.2    | `Ctrl+F` in the viewer                                             |
| JS   | @codemirror/commands        | 6.11.1   | `defaultKeymap` (cursor movement only; the doc is not editable)    |
| JS   | @lezer/highlight            | 1.2.3    | `HighlightStyle` mapping tokens to the app's colour tokens         |
| Rust | tempfile (dev only)         | 3.27.0   | tempdirs for the `fs_list` / `fs_read` tests                       |

Versions checked against npm / crates.io on 2026-09-17.

Considered and rejected:
- **`tauri-plugin-fs`** — same reasoning as `tauri-plugin-sql` in step 4: a
  capability scope per path pattern, and a generic surface when we need two
  commands. Own commands match `db.rs` / `pty.rs`.
- **Hand-picking `@codemirror/lang-javascript`, `-rust`, `-json`, `-markdown`,
  `-html`, `-css`** — `language-data` covers those plus TOML, YAML, Python,
  shell, PowerShell, Dockerfile etc. via legacy modes, and only the matched
  language's chunk is fetched (dynamic `import()`, which Vite code-splits).
- **`@codemirror/theme-one-dark`** — brings its own background and palette;
  the viewer should sit on `--panel`/`--bg` with the mockup's four token
  colours instead.
- **`ignore` crate** (gitignore-aware listing) — listing is lazy per
  directory, so `node_modules`/`target` cost nothing until expanded. Deferred;
  see decision 2.
- **`notify` crate** (file watching) — a Refresh button covers step 5; a
  watcher can arrive with editing. See decision 6.

## Rust (`src-tauri/src/fs.rs`)

Two commands, `Result<_, String>` like the rest. Both take absolute paths.

| Command   | Args   | Returns / notes                                                                 |
|-----------|--------|---------------------------------------------------------------------------------|
| `fs_list` | `dir`  | `Vec<Entry>`; directories first, then case-insensitive by name; `.git` skipped  |
| `fs_read` | `path` | `FileText`; errors `binary`, `too large: N bytes`, or the io error text         |

```rust
struct Entry { name: String, kind: &'static str /* "dir" | "file" */, size: u64 }
struct FileText { text: String, size: u64 }
```

- `fs_list`: `read_dir` → `file_type()` (a symlink to a directory counts as
  `dir` via `metadata()`; a broken link is a `file` of size 0). Names are
  `to_string_lossy`. Per-entry metadata errors drop the entry rather than
  failing the whole listing; a `read_dir` error (missing / permission) is the
  command's error and the tree shows it inline.
- `fs_read`: `metadata().len()` first — over **2 MiB** → `too large` without
  reading. Then read; a NUL byte in the first 8 KiB → `binary`. Decode with
  `String::from_utf8_lossy`, strip a leading BOM. Line endings are left as-is;
  CodeMirror's default `lineSeparator` splits on `\r\n`, `\r` and `\n`.
- **Root check** (decision 4): both commands canonicalise the path and require
  it to start with a pinned project's canonicalised root (`Db::projects_list`),
  else `"outside pinned projects: {path}"`. The webview runs only our code, so
  this is hardening, not a boundary — but it is one `starts_with` per call.
- Tests use `tempfile::tempdir()` and call the free functions directly, the way
  `db.rs` tests use `open_in_memory`; the root check is tested with an
  in-memory `Db` that has the tempdir pinned.

Registered in `lib.rs` next to `path_kind`. No capability changes.

## Frontend

### `src/fs.ts`

Thin `invoke` wrapper like `db.ts`: `fs.list(dir)`, `fs.read(path)`, plus the
`Entry` / `FileText` types.

### `src/files/tree.ts` (pure, vitest-tested)

```ts
interface Row { path: string; name: string; depth: number; kind: "dir" | "file"; expanded: boolean; state: "loaded" | "loading" | "error" }
function flatten(root: string, dirs: Map<string, DirState>, expanded: Set<string>): Row[]
```

Walks from `root`, descending only into expanded directories whose listing
has arrived, and emits rows in display order with their depth. The tree
component is then a flat list of rows — no recursive component, no per-node
state, and keyboard navigation later is index arithmetic. Paths are joined
with `joinPath` from `paths.ts`, and `samePath` keys the maps.

### `src/files/lang.ts` (pure, vitest-tested)

`languageFor(name)` → `LanguageDescription.matchFilename(languages, name)`,
with a small alias table in front for names `language-data` misses that this
machine will hit (`.mjs`/`.cjs` → JavaScript, `.tsx` → TSX, `Cargo.lock` →
TOML, `.ps1` → PowerShell, `.gitignore` → none). Returns `null` for plain text.

### `src/files/store.ts` (zustand)

Per-project state, keyed by project id, in memory only:

```ts
interface ProjectFiles {
  dirs: Map<string, DirState>;     // path → { entries } | { loading } | { error }
  expanded: Set<string>;
  open: string | null;             // absolute path
  file: FileState | null;          // { text, size } | { loading } | { error }
}
{ byProject: Map<number, ProjectFiles>;
  toggleDir(projectId, path); openFile(projectId, path); refresh(projectId); forget(projectId) }
```

- `toggleDir` on a collapsed dir marks it expanded and (re-)lists it — an
  expand always re-lists, so a directory is never more stale than its last
  open. Collapsing keeps the listing so re-expanding is instant offline.
- `openFile` sets `open` and reads; a second click on the open file re-reads.
- `refresh` re-lists the root and every expanded directory, and re-reads the
  open file. This is the only way the tree learns about new files in step 5.
- `forget` is called from `projects.remove` so an unpinned project's state
  does not linger. Switching the active project does **not** clear anything:
  A's expanded dirs and open file are still there when you come back from B.
- The root directory is listed the first time the project becomes active
  while the Files view is showing, or the first time Files is opened.

### `FilesView` (`src/components/FilesView.tsx`)

Mockup `#view-files`: toolbar + `.files` grid `270px 1fr`.

- Toolbar: `Root` · `~\<active>` (mono, `--text`) · **Refresh** button · grow ·
  hint `Read-only · Ctrl+F searches the file`.
- No active project → the grid is replaced by `.empty` "Pin a project to
  browse its files."

### `FileTree` (`src/components/FileTree.tsx`)

Renders `flatten(...)` rows as `.node` buttons (mockup styles: mono 12.5 px,
padding 4×8, radius 6, hover `--panel-2`, open file `--sel`), indented
`depth × 16px`. Directory rows: twisty `▼` rotating −90° when collapsed,
folder icon, name; a loading row shows the twisty dimmed; an error row shows
the message in `--muted` beneath the name. File rows: blank twisty, file icon,
name. Click = toggle / open. Keyboard: `↑`/`↓` move focus, `→` expands,
`←` collapses (or moves to the parent), `Enter` opens — index arithmetic over
the flat rows, ~20 lines, so it goes in now.

### `CodeViewer` (`src/components/CodeViewer.tsx`)

- One `EditorView` per mount in a `ref`'d div; the `file` state changes are
  applied with `view.setState(EditorState.create(...))`, never a remount, so
  scroll position is per state and switching files starts at the top.
- Extensions: `lineNumbers()`, `drawSelection()`, `highlightSpecialChars()`,
  `search()` + `searchKeymap`, `defaultKeymap` (movement/selection only),
  `EditorState.readOnly.of(true)`, `EditorView.editable.of(false)` (no caret
  blink, no IME), `EditorView.lineWrapping` **off** (horizontal scroll like the
  mockup's `white-space: pre`), the language extension from `languageFor`
  loaded with `desc.load()` and applied as a `Compartment` reconfigure when
  it resolves, and the theme below.
- Theme: `EditorView.theme` on the app tokens — gutter 52 px right-aligned
  `--muted` tabular-nums, content `--mono` 13 px / 1.65, `tab-size: 2`,
  selection `--sel`, search match `--shell-soft`, active-line off. Highlight
  style from the mockup: keywords `--accent`, strings `--ok`, comments
  `--muted` italic, numbers `#e3b34f` (becomes a `--num` token in
  `styles.css`), headings `--text` 600, plus `--text-2` for punctuation so
  plain text isn't stark white. Nine `tags.*` rules, no more.
- Viewer head (36 px, mockup `.viewer-head`): `<project> / <b>rel/path</b>` ·
  grow · size (`12.3 KB`) · `read-only` pill.
- States: no open file → `.viewer-empty` "Pick a file from the tree." /
  "Opens read-only; editing comes later." · loading → nothing for the first
  150 ms, then "Loading…" (reads are local, flicker is worse than a blank) ·
  `binary` → "Binary file (12.3 KB) — not shown" · `too large` → "4.1 MB —
  the viewer caps at 2 MB" · other error → the message. All three errors
  render in `.viewer-empty`, the head stays so the path is visible.
- `Ctrl+1/2/3` are not bound by CodeMirror, so the window listener from
  step 4 still sees them; `isViewShortcut` needs no change.
- The view is mounted while hidden (`visibility: hidden`, real size) so
  CodeMirror's initial measure is correct; nothing special needed there.

### `App.tsx`

The Files `<section>` body becomes `<FilesView />`. The tab hint text is
dropped. `projects.remove` calls `useFiles.getState().forget(id)`.

### `styles.css`

Adds the mockup's `.files`, `.tree`, `.node` (+ `.dir`, `.collapsed`, `.tw`,
`.open-file`), `.viewer`, `.viewer-head`, `.viewer-empty` rules and the
`--num` token. The mockup's `.code .l` line rules are not ported — CodeMirror
owns the code area.

## Tests

- Rust `cargo test` (tempdir):
  - `fs_list`: `b/`, `A/`, `.git/`, `z.txt`, `B.txt` → `[A, b, B.txt, z.txt]`
    (dirs first, case-insensitive, `.git` gone); a missing dir is an `Err`.
  - `fs_read`: UTF-8 with BOM and CRLF round-trips minus the BOM;
    `[0x00, 0x01]` → `binary`; a 2 MiB + 1 byte file → `too large`;
    a path outside the pinned tempdir → `outside pinned projects`.
- vitest `files/tree.test.ts`: `flatten` with nothing expanded gives root
  rows only at depth 0; expanding `src` inserts its rows at depth 1 directly
  after it; a loading dir yields itself with `state: "loading"` and no
  children; collapsing hides descendants but a nested expanded set survives
  re-expansion.
- vitest `files/lang.test.ts`: `App.tsx` → TSX, `db.rs` → Rust,
  `Cargo.toml` → TOML, `tasks.json` → JSON, `README.md` → Markdown,
  `.gitignore` → `null`.
- vitest `files/store.test.ts` (mocked `fs`): open a file in project 1,
  switch to 2, back to 1 → still open; `forget(1)` drops it; `toggleDir`
  twice keeps the cached entries; `refresh` re-lists only expanded dirs.

## Acceptance (manual, dev window)

1. `Ctrl+2` with `veal` active → tree shows `dist`, `mockup`, `node_modules`,
   `src`, `src-tauri`, then files A–Z (`.gitignore` first); `.git` absent;
   root reads `~\veal`; viewer says "Pick a file from the tree."
2. Expand `src` → children indented, listed instantly; expand `node_modules`
   → hundreds of rows, no stutter; collapse → gone; re-expand → instant.
3. Click `src/App.tsx` → highlighted TSX, line numbers, head
   `veal / src/App.tsx · 5.6 KB · READ-ONLY`. Typing does nothing; `Ctrl+A`,
   `Ctrl+C` copies; `Ctrl+F` opens the search panel and `Enter` steps
   through matches; `Esc` closes it.
4. `src-tauri/Cargo.toml` → TOML colours; `veal-spec.md` → headings bold,
   inline code green; `pnpm-lock.yaml` → YAML. `icons/icon.ico` → "Binary
   file — not shown". `pnpm-lock.yaml` scrolls horizontally, no wrap.
5. Arrow keys walk the tree; `→` on `layout` expands it, `←` collapses,
   `Enter` on `tree.ts` opens it.
6. Switch the active project to `hungy` → its tree, empty viewer; back to
   `veal` → `src` still expanded, `App.tsx` still open at the same scroll.
7. In a terminal pane: `ni src\zzz.ts` → not in the tree until **Refresh**
   (or collapse/expand `src`); `rm` it → Refresh drops it and, if it was open,
   the viewer shows the read error.
8. `Ctrl+1` / `Ctrl+2` from inside the viewer switches views; panes are
   untouched.
9. Unpin `hungy` → its file state is gone (re-pin shows a fresh tree).
10. `pnpm test` and `cargo test` green; `pnpm build` shows the language
    chunks split (one `javascript-*.js`, one `rust-*.js`, …), not one bundle.
11. Commit: `Add Files view: lazy directory tree and read-only CodeMirror viewer`.

## Findings during acceptance (2026-09-17)

- **A non-editable CodeMirror has no focusable content DOM.** With
  `EditorView.editable.of(false)` CodeMirror sets `contenteditable=false`
  and no `tabindex`, so clicking the viewer never focused it and `Ctrl+F`
  fell through to WebView2's own find bar. `CodeViewer` now adds
  `EditorView.contentAttributes.of({ tabindex: "0" })`, and while the Files
  view is showing a window-level `Ctrl+F` focuses the editor and opens its
  search panel, so the native bar never appears there.
- The viewer head first rendered `/src/App.tsx`; `relativeTo` now strips the
  leading separator (tested).
- Main bundle grew from 697 KB to 1054 KB (gzip 193 → 308 KB): CodeMirror
  core, search and the language index. Each language is its own chunk
  (`javascript-*.js`, `rust-*.js`, …) fetched on first open.
- The `binary` message shows no size, since the read fails before the size
  is returned; the tree row's `size` is available if that is wanted later.

## Decisions to confirm

1. **Own `fs_list` / `fs_read` commands**, not `tauri-plugin-fs`. Recommended.
2. **Lazy listing, no gitignore awareness; only `.git` is hidden.** Dotfiles
   and `node_modules` are shown. Alternative: `ignore` 0.4.33 to dim or hide
   gitignored entries — one more crate and a `.gitignore` parse per project.
3. **CodeMirror 6 + `@codemirror/language-data`** with a custom theme on the
   app tokens, rather than six hand-picked language packages or one-dark.
4. **Paths must be under a pinned project root.** Cheap hardening, one
   `starts_with` per call. Alternative: trust the webview entirely.
5. **Read cap 2 MiB; binary = NUL byte in the first 8 KiB.** Larger files show
   the size and a message rather than a partial read.
6. **No file watcher.** Refresh button + re-list on every expand. `notify`
   9.0 lands with editing, when the viewer also has to react to its own
   saves.
7. **Tree state and open file live in memory per project, not in SQLite.**
   Reopening the app starts with a collapsed tree. Alternative: persist
   `files_open:<id>` in `settings` like the layout.
8. **Read-only via `editable(false)` + `readOnly`** — selection, copy and
   `Ctrl+F` work, the caret does not blink. Editing is a later step.
9. **Keyboard navigation in the tree ships now** (arrows + Enter over the flat
   row list). Alternative: click only.
10. **Tree column fixed at 270 px** (mockup); not a draggable split.
