# Veal — plan for step 15: the review's minor findings

Status: BUILT 2026-09-17 — written and built in one go at the user's
request, after plan-12 and plan-13. Finding 10 of the 2026-09-17 review:
five small things, none of which loses data.

1. **Duplicate React keys on the Tasks error rows** (`TasksView.tsx`).
   Keyed by project *name*; two projects called `api` warn and may
   render one row.
2. **`relativeTo` is a string prefix** (`FilesView.tsx`). Root `C:\a\veal`
   and path `C:\a\veal2\x` give `2/x`. Cannot happen for a file the store
   opened (it is always under the root), but the function is shared by
   three callers and should not lie.
3. **`tilde` folds every profile** (`paths.ts`). `C:\Users\Public\x` and
   another user's tree show as `~\x`.
4. **`pathKind` has no `.catch`** (`AddProjectModal.tsx`). A rejected
   command is an unhandled rejection in the console; the hint just stays.
5. **`agentHandlers` leak after exit** (`pty.ts`). A pane left open after
   its shell exited keeps its `pty-agent` handler set until the pane is
   closed — bounded by the pane cap, but there is no reason to keep it.

## Dependencies

None.

## Changes

1. `TasksView`: key the error rows by `project?.id ?? "inbox"`; the
   `errors` entries carry the key.
2. `relativeTo`: after the case-insensitive prefix match, the next
   character must be a separator (or the path must be the root itself);
   otherwise the path is returned as it is, as for any path outside the
   root. Test: `C:\a\veal2\x` under `C:\a\veal` stays whole.
3. `paths.ts`: `tilde` folds only the *known* home. `setHome(h)` records
   it (`App.boot` calls `homeDir()` once, alongside the settings load);
   until it is known, or for a path outside it, nothing is folded.
   Matching is `samePath`-style — case-insensitive, either separator —
   and `~` alone for the home itself. `SettingsView` and
   `AddProjectModal` keep their own `homeDir()` calls (they need the
   value, not just the fold). Test: `setHome` then the old cases; a
   different profile is not folded; before `setHome`, nothing is.
4. `AddProjectModal`: `.catch(() => !stale && setKind(null))`.
5. `pty.ts`: the exit fan-out also drops `agentHandlers` for the id —
   the poller reports nothing for a pid that is gone, and the pane's own
   unsubscribe is already null-safe.

## Acceptance

1. `pnpm test`, `cargo test`, `pnpm build` green.
2. Dev window: sidebar paths still show `~\…` for pinned projects under
   the profile; pin `C:\Users\Public` → shown in full.
3. Commit: `Polish: error-row keys, strict relativeTo, tilde only for home, pathKind catch, agent handler cleanup`.

## Decisions

1. **`setHome` from boot** rather than making `tilde` async or passing
   the home through every caller (nine call sites). Alt: keep the regex
   and special-case `Public`/`Default` (still wrong for a second user).
2. **Strict `relativeTo` returns the whole path** for a non-descendant,
   as it already does for an unrelated drive. Alt: throw (three callers
   would need handling for a case that should not occur).
