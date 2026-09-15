# Veal — dev environment spec

A desktop app combining a flexible terminal multiplexer, file viewer, and task
manager, built for agentic engineering and daily dev work.

## Stack

- **Shell**: Tauri (Rust backend + system webview, not Electron)
- **Frontend**: xterm.js for terminal rendering, standard web framework for the rest of the UI
- **Backend (Rust)**: `portable-pty` for spawning and managing real PTY processes (shells and coding agents alike)
- **Persistence**: local SQLite — stores pinned projects, tasks, and pane layout so the app reopens where it left off

## Core concept

Three views, switched via a top-bar tab control: **Terminal**, **Files**, **Tasks**.
A left sidebar holds pinned projects at all times, regardless of which view is active.

## Sidebar (persistent, all views)

- List of pinned projects, each storing a root directory path
- Small indicator dot next to any project that currently has a live agent pane running, so you can tell at a glance which projects have agents active without switching to Terminal view
- "Add project" affordance at the bottom of the list
- Settings entry pinned to the bottom of the sidebar

## Terminal view

- Panes are **flexible**, not a fixed grid — each pane holds a flex-weight, not a slot, so panes can be resized by dragging dividers (like tmux/Zellij)
- No hard pane-count minimum; a reasonable soft cap (e.g. 4-6) to keep panes usable, but the layout logic itself shouldn't assume a fixed grid
- Every pane is backed by a real PTY process plus a remembered `cwd`
- **Multi-project concurrency**: panes are not scoped to "one active project" — different panes can be running in different project directories at the same time. The multiplexer doesn't need special awareness of "projects" beyond each pane's own `cwd`; it just needs to spawn any new pane inside the currently-selected project's root when the user clicks "new pane" from the sidebar.
- **Pane types — agent vs. shell**:
  - Agent panes get a **red** accent (left border stripe + a small "agent" badge in the pane header)
  - Shell panes get a **blue** accent (left border stripe + a small "shell" badge in the pane header)
  - Pane type can be set at creation (or inferred from the launched command, e.g. detecting `claude` / other agent CLIs) — worth deciding whether this is user-toggled or auto-detected
- New terminal button opens a pane already `cd`'d into the active project's directory
- Each pane header shows: type badge, project name (in monospace), close button

## Files view

- Tree view scoped to the active project's root
- Opens files in a read/syntax-highlighted viewer pane (Monaco or CodeMirror are the natural picks — both leave room to add editing later even if this starts read-only)

## Tasks view

- A simple task list/board
- Tasks can be scoped per-project or viewed globally across all pinned projects
- Persisted per-project, ideally in a way that's git-trackable (e.g. a JSON/YAML file at the project root) rather than only living in the app's SQLite — worth deciding whether SQLite is just a cache/index over these files or the source of truth

## Open decisions for Claude Code to flag back

- Exact soft cap (if any) on simultaneous panes
- Whether agent/shell pane typing is manual or auto-detected from the launched command
- Whether task data lives in SQLite only, per-project files only, or both
- Whether Files view is read-only to start or should support editing from day one
