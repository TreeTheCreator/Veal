# Veal — plan for steps 1–2: scaffold + PTY plumbing

Status: APPROVED 2026-09-16 (no installer bundling; Geist / Geist Mono fonts as in the mockup; PowerShell default shell).

## 0. Prerequisites (you run these; I can't)

Verified on this machine: Node 24.18, pnpm, WebView2 runtime 152.0.4191.66 (present).
Missing: Rust toolchain, MSVC build tools.

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install Rustlang.Rustup
rustup default stable-msvc        # in a new terminal
cargo --version                   # expect 1.9x
```

VC build tools is ~7 GB. Nothing in steps 1–2 can run until `cargo --version` works.

## 1. Scaffold — "a Tauri window opens"

### Versions (checked against npm / crates.io on 2026-09-16)

| Package                | Version | Why                                             |
|------------------------|---------|-------------------------------------------------|
| create-tauri-app       | 4.7.4   | scaffolder only, not a dependency               |
| @tauri-apps/cli        | 2.11.4  | `pnpm tauri dev/build`                          |
| @tauri-apps/api        | 2.11.1  | `invoke`, `Channel`, `listen`                   |
| tauri (crate)          | 2.11.5  | matches CLI minor                               |
| react / react-dom      | 19.3.0  |                                                 |
| vite                   | 8.3.0   |                                                 |
| zustand                | 5.0.15  | pane/layout store (used from step 3, added now) |
| typescript             | 5.x     | `strict: true`                                  |

Deliberately NOT added yet: xterm (step 2), CodeMirror (step 5), rusqlite (step 4).

### Layout

```
veal/
  veal-spec.md, plan-01-scaffold-pty.md, mockup/     (unchanged)
  package.json, pnpm-lock.yaml, vite.config.ts, tsconfig.json, index.html
  src/            React app   (main.tsx, App.tsx, styles.css)
  src-tauri/      Rust app    (Cargo.toml, tauri.conf.json, src/main.rs, src/lib.rs, capabilities/default.json, icons/)
  .gitignore      node_modules/, dist/, src-tauri/target/, src-tauri/gen/
```

`create-tauri-app` refuses a non-empty directory, so: scaffold into the scratchpad with
`pnpm create tauri-app@4.7.4 veal --template react-ts --manager pnpm`, then move the
generated files into `C:\Users\user\veal` next to the existing spec/mockup.

### tauri.conf.json numbers

- `productName: "Veal"`, `identifier: "com.veal.app"`, `version: "0.1.0"`
- window: `width 1280, height 800, minWidth 900, minHeight 600, title "Veal"`, native decorations (custom title bar is not in scope)
- dev: `devUrl http://localhost:1420` (Tauri default), `beforeDevCommand: pnpm dev`, `frontendDist: ../dist`
- `bundle.active: false` — no NSIS/MSI installer yet; keeps `tauri build` to ~1 min
- Fonts: `@fontsource/geist` + `@fontsource/geist-mono` (self-hosted woff2, imported from `main.tsx`), so the app never fetches Google Fonts at runtime like the mockup does. CSP stays at the scaffold default.

### Frontend in step 1

`App.tsx` renders the mockup's static shell only: top bar with the three tab buttons, empty sidebar, empty content area, using the mockup's CSS variables (`--bg #0c0c0d`, `--panel #111112`, `--agent #e0563e`, `--shell #7aa2ff`, ...) copied into `styles.css`. No state, no tab switching. This is a smoke test that Vite + React + Tauri wire up, not UI work.

### Acceptance

- `pnpm tauri dev` opens a 1280×800 "Veal" window showing the dark shell.
- `pnpm tauri build` produces `src-tauri/target/release/veal.exe`.
- Commit: `Scaffold Tauri 2 + React shell`.

## 2. PTY plumbing — "one real PowerShell pane in the window"

Scope: exactly one pane filling the content area, no splits, no sidebar behaviour. Flex layout and pane chrome are step 3.

### Additional dependencies

| Side | Package            | Version | Purpose                                        |
|------|--------------------|---------|------------------------------------------------|
| Rust | portable-pty       | 0.9.0   | ConPTY on Windows, openpty elsewhere           |
| Rust | serde, serde_json  | 1       | command payloads                               |
| JS   | @xterm/xterm       | 6.0.0   | renderer                                       |
| JS   | @xterm/addon-fit   | 0.11.0  | cols/rows from container size                  |
| JS   | @xterm/addon-webgl | 0.19.0  | GPU renderer; fall back to DOM on context loss |

No tokio: portable-pty is blocking, so one `std::thread` per PTY reader is the simplest correct design. At the 4-pane cap that is 4 threads. Revisit only if measured.

### Rust: `src-tauri/src/pty.rs`

State (registered with `app.manage()`):

```rust
struct PtyRegistry { next_id: AtomicU32, panes: Mutex<HashMap<u32, PtyHandle>> }
struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child:  Box<dyn Child + Send + Sync>,
}
```

Commands (all `#[tauri::command]`; app-local commands need no capability entries):

| Command      | Args                                                                              | Returns       |
|--------------|-----------------------------------------------------------------------------------|---------------|
| `pty_spawn`  | `cwd: String, cols: u16, rows: u16, shell: Option<String>, out: Channel<Vec<u8>>` | `u32` pane id |
| `pty_write`  | `id: u32, data: String`                                                           | `()`          |
| `pty_resize` | `id: u32, cols: u16, rows: u16`                                                   | `()`          |
| `pty_kill`   | `id: u32`                                                                         | `()`          |

Errors: every command returns `Result<_, String>`; unknown id → `"no pty {id}"`; spawn failure → the io error text.

Details and numbers:

- **Shell selection** (Windows): explicit `shell` arg if given; else `pwsh.exe` if on PATH, else `powershell.exe`. Non-Windows: `$SHELL` else `/bin/sh`. Env: inherit the parent's, plus `TERM=xterm-256color` (ignored by ConPTY, needed on unix).
- **Initial size**: from the frontend's first `fit()`; if the container has no layout yet, `120×30`.
- **Output path**: reader thread `read()`s into a **16 KiB** buffer and sends each chunk straight down the Tauri `Channel<Vec<u8>>` as raw bytes. It arrives in JS as a `Uint8Array` — no JSON, no base64, no UTF-8 boundary bugs, because xterm decodes the byte stream itself. No coalescing in v1; a debug counter logs chunks/sec so we can see if IPC becomes the bottleneck under agent output.
- **Exit**: reader hits EOF → `child.wait()` → emit Tauri event `pty-exit` `{ id, code: Option<i32> }` → remove the handle from the registry.
- **Kill**: `pty_kill` → `child.kill()` and drop the handle; the reader then sees EOF and `pty-exit` fires as above. Idempotent.
- **Window close**: `on_window_event(CloseRequested)` kills every registered child before exit, so no orphaned `pwsh.exe` / `claude` processes.
- **Write**: `data` is the xterm `onData` string; write bytes, flush. Large pastes go through the same call.

### Frontend

`src/pty.ts` — thin wrapper so components never call `invoke` directly:

```ts
spawnPty({ cwd, cols, rows, shell? }, onData: (bytes: Uint8Array) => void): Promise<PtySession>
PtySession = { id, write(s), resize(cols, rows), kill(), onExit(cb) }
```

`src/components/TerminalPane.tsx`:

- xterm options: `fontSize 13`, `fontFamily "Geist Mono", "Cascadia Mono", Consolas, monospace`, `scrollback 5000`, `cursorBlink true`, `allowProposedApi true` (WebGL addon needs it), theme from the mockup tokens (bg `#0c0c0d`, fg `#ececef`, cursor `#7aa2ff`, selection `rgba(255,255,255,0.07)`).
- Loads the WebGL addon; on `onContextLoss` disposes it and falls back to the default renderer.
- `ResizeObserver` on the container → `fit()`; `pty_resize` is **debounced 50 ms** and called **only when cols/rows actually changed**.
- `term.onData` → `session.write`; PTY bytes → `term.write(bytes)`.
- Unmount → `session.kill()` then `term.dispose()`.
- Spawns with `cwd = C:\Users\user\veal` (hard-coded for this step; the sidebar supplies it from step 4).
- On `pty-exit` the pane shows a one-line "process exited (code N)" footer; no auto-respawn.

### Tests

- Rust: `cargo test` spawns `cmd /c echo veal-ok` through the registry, reads to EOF, asserts the output contains `veal-ok` and the handle was removed.
- JS: `vitest` test for the resize debounce/dedupe helper — one `pty_resize` for a burst of 10 identical sizes inside 50 ms, two when the size changes.

### Acceptance (manual, in the dev window)

1. Window opens with a PowerShell prompt already in `C:\Users\user\veal`.
2. Typing works; `ls` is colored; arrow-key history works.
3. Resize the window → `$Host.UI.RawUI.WindowSize` matches xterm's cols/rows.
4. `claude` runs inside the pane with a usable TUI (the whole point of an agent pane).
5. `exit` → footer shows "process exited (code 0)".
6. Close the window → `Get-Process pwsh, claude` shows nothing left over.
7. Commit: `Add PTY spawn/write/resize/kill with a single xterm pane`.

## Decisions (2026-09-16)

- No installer bundling until much later (`bundle.active: false`).
- Fonts: Geist / Geist Mono, same as the mockup, vendored via `@fontsource`.
- Default shell: PowerShell (`pwsh.exe` if on PATH, else `powershell.exe`).
