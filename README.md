# Veal

A desktop workspace for agentic engineering on Windows: a terminal multiplexer, a file viewer/editor and a task board in one window, organised around the projects you work on.

Run Claude Code, Codex, Aider or a plain shell side by side in different projects, see at a glance which projects have an agent working, and keep each project's to-dos next to its code.

## Features

- **Pinned projects.** The sidebar keeps your project folders one click away in every view. A dot marks any project with an agent running.
- **Terminal.** Up to four real shell panes in a split layout you resize by dragging. Each pane opens in the selected project's folder, and different panes can sit in different projects at once.
- **Agent detection.** When a coding agent (`claude`, `codex`, `aider`, `gemini`, `copilot`, `amp`, `goose`, `opencode`) runs in a pane, the pane turns red and is labelled with the agent's name. Quitting Veal warns you before it ends an agent's session.
- **Files.** A live file tree for the project with a syntax-highlighted viewer and editor (Ctrl+S saves). You can create, rename (F2) and delete files; deleted files go to the Recycle Bin.
- **Tasks.** A To do / In progress / Done board per project, saved to `.veal/tasks.json` in the project so it can be committed with the code. An "All projects" view shows every board together.
- **Picks up where you left off.** Panes, open files and the current view come back when you reopen Veal.
- **Settings.** Light/dark/system theme, choice of shell (PowerShell, pwsh, cmd or WSL), and an optional proxy port passed to new terminals.
- **Automatic updates.** Veal checks for new releases on launch and installs them when you choose.

Keyboard: Ctrl+1 / Ctrl+2 / Ctrl+3 switch between Terminal, Files and Tasks.

## Install

Download `Veal_<version>_x64-setup.exe` from the [latest release](https://github.com/TreeTheCreator/Veal/releases/latest) and run it. It installs for your user only, with no admin prompt, and adds Start-menu and desktop shortcuts.

The installer isn't code-signed yet, so Windows SmartScreen may say "Windows protected your PC". Click **More info → Run anyway**.

Requires Windows 10 or 11 (64-bit). The installer fetches Microsoft's WebView2 runtime if it's missing.

## Build from source

Veal is built with [Tauri 2](https://tauri.app) (Rust) and React + TypeScript, with xterm.js for terminals and CodeMirror for the editor. You need Node.js, [pnpm](https://pnpm.io) and the Rust toolchain.

```bash
pnpm install
pnpm tauri dev      # run the app with hot reload
pnpm test           # unit tests
pnpm tauri build    # installer in src-tauri/target/release/bundle/nsis
```

### Releasing

Bump `version` in `src-tauri/tauri.conf.json`, then:

```bash
pnpm release --notes "What changed"
```

This builds and signs the installer, writes the updater's `latest.json`, and publishes both as a GitHub release (needs `gh auth login` and the updater signing key at `~/.tauri/veal-updater.key`). Add `--dry-run` to build without publishing.
