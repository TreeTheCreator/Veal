# Veal — plan for step 9: agent auto-detection

Status: APPROVED 2026-09-17 (all decisions as recommended).

Scope: the app knows when an agent is actually running in a pane. Rust
polls the process tree under every pane's shell; when a descendant is a
known agent CLI (`claude`, `codex`, `aider`, …) the pane reports it, the
badge shows the agent's name in the red accent, and the sidebar dot lights
for that project — and all of it clears when the agent exits. The manual
Shell / Agent type stays as the user's label (decision 2); detection is a
runtime overlay, never persisted. Nothing else moves: no auto-launching of
agents, no change to `pty_spawn`'s arguments, the popover or the schema.
Closes the spec's open decision "manual or auto-detected" (plan-02,
decision 1) as: **both — manual label, auto-detected truth.**

## Dependencies

- **`sysinfo = { version = "0.39", default-features = false, features =
  ["system"] }`** (Rust). One `refresh_processes` per tick gives every
  process's pid, parent, exe name and command line on Windows, macOS and
  Linux; the `system` feature alone leaves out disks, networks and
  components. Command lines matter: the npm shim for `gemini` is
  `node.exe …\gemini.js`, so exe names alone would miss it (on this
  machine `claude` is a native `claude.exe`, direct child of the shell,
  but that is one install path of several).

Nothing new on the JS side: a third `listen`, next to `pty-exit` and
`fs-changed`.

Considered and rejected:
- **Terminal title sniffing** (`term.onTitleChange` in xterm.js; Claude
  Code sets `✳ Claude Code` / the task name). Zero Rust, but each agent
  would need to set a title, and a shell that sets its own title after
  the agent exits would leave the badge stale. Process truth is truth.
- **Input sniffing** (a typed line starting with `claude`). Misses
  aliases, scripts and `npx`, and never sees the exit.
- **Toolhelp32 only** (`windows-sys`, names + parent pids). Cheaper than
  `sysinfo` and Windows-only; misses node-based CLIs (above). If the tick
  turns out to cost more than a few ms, this is the fallback.
- **Auto-flipping the persisted `kind`** when an agent is detected. It
  would make the Agent option in the popover and the `default_kind`
  setting meaningless the moment detection works, and a restored pane
  would come back "agent" with nothing running. Detection is an overlay.
- **Making the pane's *command* the agent** (`pty_spawn` with `claude` as
  the program). A different feature — auto-launch — and it removes the
  shell the user keeps after the agent exits.

## Rust (`src-tauri/src/agents.rs`)

Tauri-agnostic like `watch.rs`: `detect` is a pure function over a process
table, the walker takes root pids, and the poller takes a sink closure.

```rust
pub const AGENTS: [&str; 8] = ["claude", "codex", "aider", "gemini", "copilot", "amp", "goose", "opencode"];
pub const TICK: Duration = Duration::from_millis(1500);

/// One process, as the poller sees it.
pub struct Proc { pub pid: u32, pub ppid: Option<u32>, pub name: String /* exe stem, lowercase */, pub cmd: Vec<String> }

/// Which agent, if any, is running under `root` (the pane's shell).
pub fn detect(table: &[Proc], root: u32) -> Option<&'static str>

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneAgent { pub id: u32, pub agent: Option<String> }
```

- `detect` walks the descendants of `root` breadth-first (a `HashMap<ppid,
  Vec<&Proc>>` built once per tick, so the walk is linear in the subtree)
  and returns the first agent it meets, nearest first. A process matches
  when its **exe stem** is in `AGENTS` (`claude.exe` → `claude`), or when
  its stem is an interpreter (`node`, `bun`, `deno`, `python`, `python3`)
  and any **command-line argument's file stem** is in `AGENTS`
  (`…\gemini.js`, `…\aider\__main__.py`). Windows PIDs are reused, so a
  parent pid that points at a process *newer* than its child is ignored
  — `sysinfo` exposes `start_time()` for this.
- `Poller::start(registry: Arc<PtyRegistry>, sink: impl Fn(PaneAgent))`
  spawns one thread. Every `TICK`: if the registry has no panes, sleep and
  skip the refresh; else `refresh_processes_specifics(ProcessesToUpdate::
  All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::
  OnlyIfNotSet))` — command lines are read once per process, not per
  tick — build the table, run `detect` for each `(pane id, shell pid)`
  from `registry.pids()`, and call `sink` **only when a pane's answer
  changed** since the last tick (a `HashMap<u32, Option<&str>>` of last
  answers; a pane that disappears is dropped from it). Nothing is sent for
  a pane that has always been `None`.
- `PtyRegistry` stores the shell's pid (`child.process_id()`) in
  `PtyHandle` and gains `pub fn pids(&self) -> Vec<(u32, u32)>`.

`lib.rs`: `setup` starts the poller with `sink = move |a| app.emit("pty-
agent", a)`. No new commands.

Cost: `refresh_processes` on this machine (~300 processes) is a few
milliseconds on a background thread every 1.5 s; the acceptance run
measures it (item 7).

## Frontend

### `src/pty.ts`

`session.onAgent(cb: (agent: string | null) => void)` on `PtySession`, fanned
out from one `pty-agent` listener by PTY id like `onExit`. *Amended during
acceptance — see findings: the event carries the PTY id, and only the
pane's `TerminalPane` knows which pane that is.*

### `src/layout/store.ts`

```ts
/** Detected agent per pane id; runtime only, never in the snapshot. */
agents: Map<number, string>;
setAgent(id: number, agent: string | null): void;
```

`setAgent` copies the map; `closePane` drops the id. The tree is
untouched, so `persist.ts`'s subscribe (which compares `tree`, `focused`,
`nextId`) never fires for a detection change and nothing new is saved.

### `TerminalPane`

Props gain `agent: string | null`. The pane's class becomes
`pane ${agent ? "agent" : kind}` — a detected agent is red whatever the
label. The badge reads `agent ?? kind`: `claude` in the agent accent while
Claude runs, then back to `shell` (or `agent`, if that is the label). The
flip button still flips the label; `title` becomes `claude running · click
to flip the label` while detected.

### `Sidebar` / `projects/store.ts`

`liveProjectIds(tree, projects, agents)` — a project is live when a pane
whose `cwd` is its root has a **detected** agent. The manual label no
longer lights the dot (decision 3); its `title` becomes `claude running`.

### `SettingsView`

A read-only row after **Pane soft cap**: **Agent detection** — "Badge and
sidebar dot follow the process tree, polled every 1.5 s" · mono
`claude · codex · aider · gemini · copilot · amp · goose · opencode`.

### `TypePopover`

The Agent option's hint becomes `Label only — the badge follows what
actually runs`.

## Tests

- Rust `cargo test` (`agents.rs`):
  - `detect` on a hand-built table: `pwsh(1) → claude.exe(2)` → `claude`;
    `pwsh(1) → cmd(2) → node(3, cmd ["node", "…\\gemini.js"])` → `gemini`;
    `pwsh(1) → node(2, cmd ["node", "…\\vite.js"])` → `None`; `pwsh(1)`
    alone → `None`; an agent under a *different* root → `None`; two
    agents → the nearer one; a `ppid` pointing at a process that started
    after its child is not followed (pid reuse).
  - Interpreter rule needs the stem exactly (`nodemon` is not `node`;
    `claude-helper.exe` is not `claude`).
  - Integration (real PTY, like `pty.rs`'s test): spawn `cmd.exe`, type
    `ping -n 30 127.0.0.1`, build the table from `sysinfo` and assert
    `descendants(shell pid)` contains a `ping`; `exit` after. Marks the
    walker works with real pids and `sysinfo`'s parent links on Windows.
  - Poller change-only rule, with a fake table source: `None → Some →
    Some → None` over four ticks emits exactly two events.
- vitest `layout/store.test.ts`: `setAgent` adds and clears; `closePane`
  drops the entry; a detection change does not change `tree` identity.
- vitest `projects/live.test.ts`: dot follows `agents`, not `kind`: a
  manual agent pane with no detection is not live; a shell pane with
  `claude` detected at the project root is.
- vitest `layout/persist.test.ts`: `serializeLayout` output has no
  `agents` key.

## Acceptance (manual, dev window)

1. Open a shell pane in `veal`, run `claude` → within ~1.5 s the badge
   reads `claude` in red, the pane's left stripe is red, the sidebar dot
   for `veal` lights with title `claude running`. `Ctrl+C` / `/exit` → badge
   back to `shell`, stripe blue, dot off.
2. Same with `npx @google/gemini-cli` (or any node-shim CLI on the
   machine) → `gemini` — exercises the command-line rule.
3. Set the label to Agent (flip) with nothing running → red stripe, badge
   `agent`, **no** dot. Run `claude` → badge `claude`, dot on. Exit → badge
   `agent` (label kept), dot off.
4. Two panes in `veal`, agent in one → one dot. Agent in a pane whose cwd
   is `veal\src` (cd first) → no dot (root-cwd rule, unchanged).
5. Close a pane while `claude` runs → dot off at once (`closePane` drops
   the entry; the PTY kill ends the process).
6. Restart with restore on while an agent was running → panes come back
   as fresh shells, no badge, no dot (nothing persisted).
7. Task Manager: `veal.exe` CPU stays at 0 % when idle with four shell
   panes; the Rust log (debug build) prints the tick's refresh time —
   expect single-digit ms.
8. `pnpm test`, `cargo test`, `pnpm build` green.
9. Commit: `Add agent auto-detection: process-tree polling, live badge and sidebar dot`.

## Findings during acceptance (2026-09-17)

- **The PTY id is not the pane id.** Rust's `pty-agent` carries the
  registry's PTY id; the layout store keys panes by its own ids. The plan's
  module-level `onAgent(cb)` would have filed detections under the wrong
  pane (the first run showed `agents: [[4,"claude"],[6,"claude"]]` for
  pane 3). It is now `session.onAgent(cb)` on `PtySession`, fanned out by
  PTY id exactly like `onExit`, and `TerminalPane` — which owns both ids —
  calls `setAgent(paneId, agent)`. No `startAgentDetection()`; the
  listener starts with the first spawn.
- Acceptance 1, 3, 4, 5, 7 ran against the dev window over CDP (the same
  driver as plan-07, plus `window.__veal.panes` to type into a pane):
  `claude` in a shell pane → badge `claude`, red stripe, `veal` dot titled
  `claude running`, one dot only; flipping the label while it runs keeps
  the badge; after exit the badge returns to the label and the dot goes
  out; a pane in `veal\src` running `claude` gets the badge but no dot;
  closing a pane mid-run drops its entry.
- Acceptance 2 with a stand-in: `node mockup\gemini.js` (a 5 s script) →
  `gemini` **668–1564 ms** after Enter; cleared **1.8 s** after the script
  exited. No node-based agent CLI is installed here, so the interpreter
  rule was exercised with the file name only.
- `refresh_processes` costs **~22 ms** per tick for ~265 processes in the
  debug build (the log threshold was raised to 50 ms so the dev log stays
  quiet); `veal.exe` CPU over 10 s idle with panes open: 16 ms (0.2 %).
- Writing `\x03` to the PTY from the driver does not interrupt `node`
  under ConPTY (Claude Code, which handles Ctrl+C itself, exits fine);
  the acceptance script lets the stand-in exit on its own instead. Not a
  detection issue.
- Not run: 6 (restart with an agent running) — covered by the unit test
  that the snapshot never carries `agents`.

## Decisions to confirm

1. **Detection by process tree, polled every 1.5 s in Rust with
   `sysinfo`**, events only on change. Alternative: terminal-title
   sniffing in JS (no Rust, fragile), or a 1 s tick.
2. **The manual Shell / Agent label stays; detection is a runtime overlay
   that wins for the accent and badge text.** Alternative: drop the manual
   type entirely (popover, `default_kind`, flip) and derive everything.
3. **The sidebar dot means a detected agent, not the label.** Alternative:
   label *or* detection (today's behaviour plus detection).
4. **Agent list is a built-in constant of eight**: `claude codex aider
   gemini copilot amp goose opencode`. Alternative: an editable setting
   (comma-separated) with these as the default.
5. **Match rule: exe stem, or interpreter + an argument's file stem.**
   Alternative: substring anywhere in the command line (catches more,
   also catches `claude-notes.txt` in an editor's arguments).
6. **Nearest agent wins when two are nested** (an agent that spawns
   another agent). Alternative: the outermost.
7. **Badge shows the agent's name** (`claude`) rather than the word
   `agent`. Alternative: keep `agent` and put the name in the title.
8. **Read-only Settings row listing the agents.** Alternative: none.
