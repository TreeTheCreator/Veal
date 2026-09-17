# Veal — plan for step 11: editing task titles

Status: BUILT 2026-09-17 — written and built in one go at the user's
request; the decisions at the end are the ones the build follows.

Scope: a task's title can be changed from the board. Double-click the
title (or the card's new ✎ button) and it becomes an inline input, the
same interaction as the Files tree's rename (plan-09): Enter commits,
Escape and blur cancel, an empty title cannot be committed. The write is
one more Rust read-modify-write under the tasks lock — `.veal/tasks.json`
for a project, `veal.db` for the Inbox — so an agent's concurrent edit to
the file is merged, not clobbered, and the watcher (plan-07) reloads the
board as it does for any other write. Nothing else moves: no reordering,
no ◂ back-step (the other two parts of plan-05's decision 9 stay open), no
editing of `created` or the extras an agent may have put on a task.

## Dependencies

None.

Considered and rejected:
- **Editing other fields** (`created`, notes). `created` is a fact, not a
  setting; extras are the agent's, shown nowhere, and the file is the
  place to edit them.
- **A modal** — same reasoning as plan-09: one field, keep the eye on the
  card.
- **Autosave on every keystroke.** A title is one write; agents watch the
  file; commit once on Enter.

## Rust

- `tasks::set_title(root, id, title)` in `tasks.rs`: trim, refuse empty
  (`empty title`, as `add` does), unknown id → `no task "<id>"`, rewrite
  the file. Status, `created` and extras untouched.
- `Db::inbox_set_title(id, title)` in `db.rs`: same rules, `UPDATE tasks
  SET title`.
- Command `tasks_set_title(root, id, title)` in `lib.rs` under
  `TasksLock`, returning the whole list like the others.

## Frontend

- `tasks/api.ts`: `tasks.setTitle(root, id, title)`.
- `tasks/store.ts`: `setTitle(projectId, root, id, title)` through
  `apply` (reload on error, like `setStatus`).
- **`components/InlineInput.tsx`** — the Files tree's `NameInput` moves
  out of `FileTree` and becomes shared: `initial`, `validate(value):
  string | null`, `selectTo(initial): number`, `onCommit(value):
  Promise<boolean>`, `onCancel()`, `className`. `FileTree` uses it with
  `validateName` / `stemLength`; the task card uses it with "empty title"
  and select-all.
- `TaskCard`: the title is a `<div className="title">` with
  `onDoubleClick`; the actions gain a `✎` (title "Edit title") before
  ▸/↺/×. While editing, the title is the `InlineInput` (`.task
  input.title-edit`), the actions are hidden, and the card gets
  `.editing`. An unchanged title just cancels. Only one card edits at a
  time (`editingKey` state in `TasksView`, keyed `project:id`).
- Hint in the toolbar becomes `▸ moves a task forward · double-click a
  title to edit it`.

## Tests

- Rust (`tasks.rs`, tempdir): `set_title` rewrites the title and keeps
  status / created / extras; trims; empty → `empty title` and the file is
  untouched; unknown id → error. `db.rs`: `inbox_set_title` same.
- vitest `tasks/store.test.ts`: `setTitle` calls the API and replaces the
  list; a failure records the error and reloads.
- vitest `files/store.test.ts` unchanged (the input moved, not its
  behaviour).

## Acceptance (manual, dev window)

1. Double-click a task title → input with the whole title selected; type
   a new one, Enter → the card shows it, `.veal/tasks.json` on disk has
   it, status and `created` unchanged. `git diff` is one line.
2. ✎ on a card → same input. Escape → old title back, file untouched.
   Click elsewhere → same.
3. Delete the whole title, Enter → nothing happens, red outline; Escape.
4. Inbox task in global scope → same, and `veal.db` has the new title
   after a restart.
5. While an input is open, `Add-Content` a new task to the file from a
   pane → the board gains the card, the input stays open with its text.
6. `pnpm test`, `cargo test`, `pnpm build` green.
7. Commit: `Add inline task title editing`.

## Findings during acceptance (2026-09-17)

- Acceptance 1–5 ran in the dev window with real double-clicks, typing
  and keys over CDP: 13 checks, all passing. The seeded tasks and the
  Inbox task were removed afterwards; the run created `veal\.veal\` and
  removed it again.
- Inserting a task into `tasks.json` from outside while an input was open
  reloaded the board (the watcher) without disturbing the input — the card
  keeps its identity by `project:id`, so React leaves the field alone.
- The Files tree's `NameInput` became `components/InlineInput.tsx` with
  `validate` / `selectTo` props; `FileTree` behaviour is unchanged and its
  tests still pass untouched.
- Nothing found to fix.

## Decisions (built as recommended)

1. **Double-click the title or click ✎; inline input; Enter commits,
   Escape/blur cancel** — the plan-09 interaction. Alternative: click to
   edit (too easy to trigger while reaching for ▸).
2. **Empty titles are refused inline** (red outline, Enter no-op), Rust
   refuses too. Alternative: commit as a delete.
3. **The inline input is one shared component** for the tree and the
   board. Alternative: a second copy in `TaskCard`.
4. **Reordering and ◂ back-step stay out** — they are the next step if
   wanted. Alternative: fold ◂ in now (one button, one `backward()`).
