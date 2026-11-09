# Veal — plan for step 20: Shells live in a background daemon

Status: build order 1 (the gate) RUN 2026-11-07: passed, with the conditions
below. Step 2 BUILT and checked 2026-11-07. Step 3 BUILT 2026-11-08, with two
plan-15 checks left for a person (below). Step 4 BUILT 2026-11-08. Steps 5–8 not
started.

Results for 4: panes carry a UUID `session` key and `pty_open` attaches to it or
spawns it, so F5 keeps every shell. `pty_spawn` and `pty_reset` are gone. Rust's
`on_page_load` (Started) detaches every client, standing in for a dropped
connection until step 5. Boot runs `reconcile` (one test per row of section 5), and a
layout subscriber is the only thing that kills. `MAX_SESSIONS` = `PANE_CAP`, by test.
The newest responder takes the role, so a reload or React's double effect answers a
query once. `cargo test --workspace` 123 (+2 ignored): 42 app, 81 core. `pnpm test`
229. Clippy has the same single warning. Installer 5.06 MB.

Live, on the release build over CDP (`Page.reload`, which is what F5 does):

| Check | Result |
|---|---|
| F5 with a child counting to a file | Same shell PIDs, the child never stopped, screen restored, twice in a row |
| Input after reattach | Ctrl+C stops the child, typing works |
| Close a pane, then F5 | Only its shell dies, and it stays closed |
| Agent across F5 | Badge shows at once after the reload, clears when the agent exits |
| `exit 0` during a reload | Pane closes |
| `exit 3` during a reload | `process exited (code 3)` and the last screen, on every later reload too, until the pane is closed |
| F5 30–90 ms after a new pane | 0 of 30 shells stuck (was 1 of 12, see below) |
| Close the app | Exits, no shells left; relaunch restores the layout with fresh shells |

Three things the live run found, all fixed with tests:

- A tombstone was dropped once its last client had drained it, so a second F5
  started a fresh shell in a pane that had just said `exit 3`. A session opened by
  key now stays until its pane closes (`kill`) or boot finds no pane for it
  (`forget`), against section 2's "until a client has seen it".
- ConPTY's opening cursor query, sent to a page that is then reloaded, was never
  answered, and the shell hung for good: section 3's known gap, but for the one
  query that cannot time out. The session now remembers answers it left to a
  responder until the child's next output, and sends them itself if no caught-up
  responder is left.
- Sending that answer while the old page's own answer was still in flight gave
  conhost two, and the second `ESC[1;1R` is xterm's F3, which PowerShell binds to
  a search that swallows the next key. The owed answer now waits 250 ms and is
  dropped if a client's input carries a cursor report.

Open: once, after a long run of these scripts in which the CDP harness itself
also broke, closing the app left `veal.exe` running with no window and its shell
alive: `kill_all` did not run, or blocked. Idle, so a wait, not a loop. Not
reproduced in three more instances running the same scripts, nor with no debugger to
see where it stopped. Watch for it; step 6 replaces this close path anyway.
Console noise on reload, harmless: Tauri's "IPC custom protocol failed" and
"Couldn't find callback id" for calls the old page left in flight, and
`pty_write`/`pty_resize` "no pty" from an exited pane.

Results for 3: `core/session.rs` holds the emulator, the serializer, a queue per
client, query answering and tombstones. The reader no longer blocks, and a client
past `OUTBOX_CAP` gets a snapshot on its next take. A new client starts the same way,
so there is one snapshot path. The app still uses one client per pane; `attach`,
`detach`, `forget` and `set_theme` exist in core and are tested, unused until steps 4–5.
`cargo test --workspace` 109 (+2 ignored): 42 app, 67 core. Clippy has the same single
warning. The installer went from 4.19 MB to 5.04 MB, for the emulator and
`panic = "unwind"` together; they were not measured apart. `scripts/gate` now
drives the shipped `Session`, not a copy.

G1 repeated, on a real `claude` session of three turns with Glob, Read and Grep
(124 KB). It found a bug the short sessions could not: 47 of 750 cuts lost a blank
row in the alternate screen, moving everything below it up one, until the next
redraw. A full-screen app can leave a row's wrap flag set after erasing the row below.
Fixed, with a test: now 750 of 750 exact, at the cut and after continuing, 0 replies.
Re-run on the fix: `styles` and `scrollback` 40 of 40 exact, 327 KB and 56 ms for
5000 lines. `cls` was exact at all 40 cuts, and 28 ended with only the saved cursor
row differing, the gap listed below. `shell-resize` is still the accepted G6 drift,
measured before the fix and not re-run. Cost before the fix: 38 MB/s, 0.5% of a core
at 195 KB/s, 16 MB. Still one session on one machine.

plan-15's list on the release build, over CDP:

| Check | Result |
|---|---|
| A1 | `pnpm test` 201, `cargo test`, `pnpm build` green |
| A2 One pane flood | Ctrl+C to a working prompt in 326 ms |
| A3 Four panes, 60 s | None stopped. Private bytes flat at 64 MB, UI answers in 2–6 ms, Ctrl+C in 129–243 ms each |
| A4 Three `claude` panes, 20 minutes | **Not run: needs a person** |
| A5 Wheel scrolling, Ink frame whole | **Not run: needs a person** |
| A6 `exit` in a flooding pane | Closes it |
| Page paused during a flood (acceptance 10) | Child finished 150,000 lines with the debugger paused throughout, and the screen was correct on resume, ending in the right lines and prompt. So the WebGL renderer takes a real snapshot |

Emoji and CJK draw at the right width with the Unicode 11 addon, in one screenshot.

Moved out of this step, against section 2: `CreateEnvironmentBlock` and the dev overlay go
to step 5, since only a long-lived daemon can hold a stale `PATH`, and until then
a fresh block would only lose a dev shell's variables. Reporting the theme goes to
step 5 with the protocol's `theme` message. Headless query answers are live, but in the
app nothing runs detached until step 4. A query in bytes dropped by an overflow
already queued is lost, like the responder-disconnect gap in section 3.

Results for 2: `pty.rs`, `agents.rs`, `proxy.rs` and `sysproxy.rs` moved
unchanged to `src-tauri/core` (`veal-core`). `src-tauri/Cargo.toml` is now the
workspace root as well, so `cargo metadata` still gives `src-tauri/target`.
`cargo test --workspace` 83 (+2 ignored) as before, split 42 app and 41 core.
Clippy has the same single warning, `tsc` is clean, and the lock file gained
`veal-core` with no version changes. `pnpm tauri build` produced a signed installer
of 4.19 MB, about the same as the last one (built from older code), but into a
scratch target folder, because a running Veal had `target\release\veal.exe` locked.
With Veal closed, both remaining checks then passed: `release.mjs --dry-run` built
and signed the installer in the normal target folder, and the release `veal.exe`
opened a pane whose PowerShell ran a typed `echo`. Moved out of this step, against section 1:
`panic = "unwind"` goes to step 3, because until `catch_unwind` exists a panic in a
reader thread would leave a frozen pane instead of ending the app. The logger and
the `veal-daemon` crate go to step 5, because nothing uses them before then.

Gate results (harness now in `scripts/gate`, outside the workspace). Real ConPTY streams were recorded with a headless xterm 6.0 answering queries,
then replayed and cut at random byte offsets. Each snapshot went into a fresh
xterm and was compared cell by cell with a control that saw the whole stream.
Both then got the rest of the stream and were compared again. Winner:
`alacritty_terminal` 0.26 with our own serializer. `vt100` matched 0 of 150 cuts
(it lost alternate screen, focus mode, title). A raw replay drew the screen
correctly on short streams, but made xterm send replies at 150 of 150 cuts.

| Check | Result | Evidence |
|---|---|---|
| G1 Real `claude` | Pass | 750 of 750 cuts exact, at the cut and after continuing (3 seeds). Real xterm with the DOM renderer: 4 of 4 cuts pixel-identical. |
| G2 Modes | Pass, with gaps | Through ConPTY, 40 of 40 exact. A hand-built stream sent directly was exact at 186 of 200 cuts (see gaps below). |
| G3 Scrollback | Pass | 7000 lines printed, 5000 kept: exact at every cut, with colours. 327 KB, 55 ms. |
| G4 No replies | Pass | 0 replies from any `alacritty_terminal` snapshot. |
| G5 Reset on a used xterm | Pass after a fix | 7 cases, including mid-CSI, OSC, DCS, ESC and UTF-8. `ESC c` does not clear a hidden cursor, so a snapshot must also send `ESC [ ? 25 h`. |
| G6 Resize | Partial | See below. |
| G7 Cost | Pass | 32–64 MB/s, so 0.3–0.6% of a core at 195 KB/s. 16 MB for 5000 lines. Snapshot about 90 ms at 5000 lines. |

G6 in detail. The visible screen always converged: in a plain shell, the only
differences were at 27 of 165 cuts, all within 3 output events of a resize, and
ConPTY's repaint cleared them. In `claude`, the alternate screen was exact at all
162 cuts; 20 cuts differed in the hidden main screen, which after exit comes down
to one scrollback line wrapped differently. Scrollback loses a little more than
xterm does: after 5 resizes, printed output of 120 known lines survived as 117 in
the control and 112 in the emulator, one of those being two lines run together.
The emulator's own
grid shows the same, so the serializer is not the cause. xterm loses lines here
too, so today's Veal already has a smaller version of this. Accepted: it needs
repeated resizes, and affects only scrollback.

Gaps found by the hand-built stream, none of which occurred in the four real
streams scanned (`claude`, `claude-resize`, `shell-resize`, `cls`): blink and
overline are not kept by the emulator, the scroll region cannot be read from it,
saved-cursor position can differ, and `ED 2` pushes the screen into scrollback
where xterm does not. A program that uses these could restore imperfectly.

Limits of this evidence: two short `claude` sessions (25 KB and 38 KB, one answer
each, no tool use) on one machine, offline replay rather than a live daemon, and
the WebGL renderer was not checked. Step 3 should repeat G1 on a long session
with tool use. Seen in both the control and the restored terminal, so not a
restore problem: late in the `claude` recording some list lines appear twice on
screen. Not investigated; worth checking whether a Veal pane does the same.

Corrections to this plan from the gate: G6 below wrongly said xterm does not
reflow here. At build 26200 it reflows the main screen, and only the alternate
screen is left alone (`Buffer.ts:296`). The comment at `TerminalPane.tsx:62` says
the same wrong thing. Sections 2 and Dependencies are updated.

Scope: a new process, `veal-daemon.exe`, owns every shell. The desktop app
becomes a client that attaches to it. Closing the window, reloading the page
or crashing the UI no longer ends a running agent, and reopening Veal brings
the same panes back with their screen and scrollback.

Why: the app's lifetime and the agents' lifetime are the same today. Closing
the window kills everything (`lib.rs:421`), a page reload kills everything
(`App.tsx:42`, `pty.ts:136`), and a restored layout only respawns fresh
shells (`layout/persist.ts:2`). This step separates them, and gives remote
access (plan-20) one place to connect to.

Not in this step: the phone app, network listeners, pairing, notifications,
starting at login, agent working/waiting state. Laptop sleep is not solved
by a daemon. Those belong to plan-20. This step only keeps plan-20 possible:
the protocol does not assume a pipe, and a client can attach without
answering terminal queries or controlling the size.

## What stays where

| Daemon (`veal-daemon.exe`) | Desktop app (`veal.exe`) |
|---|---|
| Shells (`pty.rs`) | Rendering (xterm), pane arrangement |
| Screen state and scrollback per session | SQLite: settings, projects, layout |
| Fan-out of output to clients | Files, Tasks, the watcher |
| Agent detection (`agents.rs`) | Updater, dialogs, close guard |
| Proxy resolution at spawn (`proxy.rs`, `sysproxy.rs`) | Choosing spawn arguments |
| Tray icon | |

The daemon has no database. The desktop app already sends the cwd, shell and
proxy settings with each spawn, and in this step nothing else spawns. So
there are no schema migrations in the daemon and no two-writer SQLite. When
plan-20 needs a spawn with no window open, the daemon can open the database
read-only then.

## 0. Gate: restoring a screen (do this first, outside the repo)

Everything depends on this. A throwaway program in the scratchpad: spawn a
ConPTY with `portable-pty`, feed the output to a candidate, snapshot it at a
random moment, and write the snapshot into a fresh xterm.js 6.0 terminal
(dev window over CDP, the `__veal` hook style from `TerminalPane.tsx:146`).

Candidates, in the order to try:

1. `alacritty_terminal` 0.26. A full emulator with scrollback, and it emits
   `Event::PtyWrite` for query replies. It has no serializer, so we write
   one: grid to ANSI, about 200–300 lines. Most likely winner.
2. `vt100` 0.18. Has formatted screen output built in, so it is less work.
   To confirm: whether scrollback and modes come through.
3. Raw byte ring buffer plus a resize nudge to force a redraw. Least code,
   and included as the baseline. Expected to fail: replaying raw bytes
   replays the queries too, and xterm answers them (`InputHandler.ts:2663`),
   which types junk into the agent.

Pass means all of these, compared cell by cell against a control xterm that
saw the whole stream, plus by eye:

- G1. Real `claude` session, idle and mid-redraw: the screen is identical
  after restore.
- G2. Modes survive: alternate screen, bracketed paste, mouse tracking,
  cursor visibility and shape, application cursor keys, title.
- G3. 5000 lines of scrollback come back, in order, with colours.
- G4. A snapshot makes xterm send nothing back on `onData`.
- G5. The snapshot's leading reset (`ESC c`) clears the old scrollback in
  xterm.js. If it does not, section 7 needs a real reset call.
- G6. Resize after restore. xterm and `alacritty_terminal` reflow differently
  (corrected above). The two must not drift apart once ConPTY repaints.
- G7. Cost at plan-15's heaviest flood (four panes, about 195 KB/s rendered
  each): parsing keeps up with less than 5% of a core per pane in release.
  At 5000 lines, under 50 MB per pane.

If no candidate passes G1 and G4, stop and rethink before writing any of the
below. Fallback to discuss then: snapshots from the desktop's own xterm with
`@xterm/addon-serialize`, which needs a window and so gives up a headless
restore for plan-20.

## 1. Crates

`src-tauri/Cargo.toml` becomes the workspace root as well as the app, so
`src-tauri/target` stays where `release.mjs:41` looks for it.

- `src-tauri/core` (`veal-core`, no Tauri): `pty.rs`, `agents.rs`,
  `proxy.rs`, `sysproxy.rs`, new `session.rs`, `protocol.rs`, `client.rs`.
- `src-tauri/daemon` (`veal-daemon`, a bin): the listener, lifecycle, tray.
- `src-tauri` (`veal`): Tauri, `db.rs`, `fs.rs`, `tasks.rs`, `watch.rs`.

Rejected: `veal.exe --daemon`. It links WebView2 and Tauri into a process
that needs neither, and the installer has to replace the very file the live
daemon is running from, with the name its running-app check looks for.

`panic` moves from `abort` to `unwind` in the release profile (it cannot be
set per package). Today a panic costs one window. In the daemon, `abort`
would end every agent because of one bad byte sequence. The emulator call is
wrapped in `catch_unwind`: if it panics, that session stops offering
snapshots and its shell keeps running. Check the size cost in the installer.

The daemon never uses `eprintln!`. plan-18 recorded a pty reader thread
dying on one when stderr had gone away. It logs to a rotating file under
`%LOCALAPPDATA%\com.veal.app\logs`. The debug-only lines in `pty.rs:284` and
`agents.rs:200` go through the same logger.

## 2. Sessions (`core/session.rs`)

The reader thread stops feeding an `Outbox`. It now does this, under the
session's lock:

```
read bytes -> emulator.advance -> answer queries if no responder -> push to each client's queue
```

The queue push never blocks, so snapshots and fan-out are always consistent:
a client attaches under the same lock, gets a snapshot, and every later byte
follows it with no gap or overlap.

**What a snapshot must do (each one found by the gate).**

- Start with `ESC c ESC [ ? 25 h`, because the reset leaves a hidden cursor hidden.
- End with any partial sequence the parser is holding. A read can stop inside an
  escape sequence or UTF-8 character, the emulator keeps that state, and a client
  that lacks it prints the remainder as text. Without this, 2 of 150 cuts printed
  stray characters; with it, none did. A small state machine in the reader tracks
  those bytes.
- Reach the hidden main grid with `swap_alt`, since `inactive_grid` is private.
  Swapping back wipes the alternate grid, so clone it first and restore it.
- Recreate a pending wrap by reprinting the last cell, and send a cursor column
  move after non-ASCII glyphs, so one width disagreement cannot shift a whole row.

**Glyph widths.** Veal's xterm uses its default Unicode 6 table, where an emoji is
1 column wide and the emulator says 2. So load `@xterm/addon-unicode11` in
`TerminalPane`: with it, the styles stream went from 11 of 40 exact to 40 of 40.
This changes how live panes draw emoji, so check it by hand. Emoji added after
Unicode 11 still disagree and come back with an extra gap until redrawn, which
was accepted. `addon-unicode-graphemes` was tried and is not a fix: it clusters
sequences such as `⚠️` that the emulator does not, so that row disagrees instead.

**Backpressure changes (replaces plan-15's chain).** Today an unread pane
blocks its child at `OUTBOX_CAP`. With a daemon that would let a frozen
window or a bad phone connection stall an agent. New rule: the child is only
ever limited by the emulator. Each client has a queue per session, capped at
`OUTBOX_CAP`. On overflow the queue is dropped and the client gets a fresh
snapshot when it next catches up. Up to 5000 lines of scrollback survive
this because the snapshot carries them. Re-run plan-15's acceptance list.

**Exited sessions.** A session whose child exits with nobody attached stays
as a tombstone (key and exit code) until its pane closes, boot finds no pane
for it, or the daemon exits. (Step 4: "until a client has seen it" lost the
message on a second reload.) Without this, code 0 while detached could not close the pane and a
crash could not show `process exited`.

**Environment.** A long-lived daemon would hand new panes a stale `PATH`
after the user installs a tool. Each spawn builds a fresh user environment
with `CreateEnvironmentBlock`, then adds `TERM`, `VEAL_PANE` and the proxy
variables as today. A dev run started from a shell loses that shell's own
variables, so the spawn message carries an optional overlay for dev.

## 3. Terminal queries: exactly one answer

ConPTY opens with `ESC [ 6 n` and stalls all output until something answers
(`TerminalPane.tsx:133`). Today xterm answers. With a daemon there can be no
listener (window closed) or two (desktop and phone), and two listeners both
answering means junk typed into the agent.

- A client attaches as `responder: true` or `false`. At most one responder
  per session; the desktop is one.
- While a responder is attached, the daemon forwards queries and ignores the
  emulator's `PtyWrite`, so agents get the same answers from xterm as today.
- With none attached, the daemon answers from the emulator's `PtyWrite`,
  plus OSC 10/11 from the last theme a client reported.
- A non-responder client sets xterm's `disableStdin`, which drops every
  reply at `CoreService.ts:63`, and sends input with explicit `write`
  messages. That is plan-20's phone client; the rule is fixed now so the
  protocol does not change later.

Rejected: the daemon always answers and every xterm stays silent. Cleaner,
but agents would then see the emulator's capabilities instead of xterm's. An
agent that detects kitty keyboard support would switch modes and xterm would
then mis-encode its keys.

Known gap: a query sent to a responder that disconnects before answering is
lost. See A9. Except when the child prints nothing after it, which is ConPTY
waiting on its opening query: then the session sends the answer itself, 250 ms
later, unless a client's input carried a cursor report (step 4).

## 4. Protocol (`core/protocol.rs`)

Frames are `u32 length`, `u8 kind`, payload. Control messages are JSON;
output is raw bytes with a session number. Nothing in it is specific to a
pipe, so plan-20 can carry the same messages over a WebSocket.

Client to daemon:

| Message | Notes |
|---|---|
| `hello { protocol, build, role }` | First message. Mismatch is section 8. |
| `list` | Sessions and tombstones: key, cwd, pid, agent, exit code, size. |
| `spawn { key, cwd, cols, rows, shell, proxy…, env? }` | Idempotent by key. At the cap: an error. |
| `attach { key, cols, rows, responder, controller }` | Resize if controller, then snapshot, atomically. Reply includes the current agent. |
| `detach { key }` | Session keeps running. |
| `write { key, bytes }` | One message is written whole, so a paste is not interleaved. |
| `resize { key, cols, rows }` | Controller only. |
| `kill { key }`, `forget { key }` | Kill a session; drop a tombstone. |
| `theme { fg, bg }` | For OSC 10/11 while headless. |
| `shutdown` | Kill everything and exit. |

Daemon to client: `output`, `snapshot`, `exit`, `agent`, `session_added`,
`session_removed`.

**`attach` must report the agent.** `agents::tick` only reports changes
(`agents.rs:153`), so a client attaching to a running `claude` would
otherwise show a blue `shell` pane and the close guard would see no agents.

## 5. Keys, panes and the layout

Pane ids and PTY ids are separate today, and only the layout survives a
restart. The link between them:

- `PaneNode` gains `session: string`, a UUID created by `addPane`.
  `parseNode` gives an old snapshot's pane a new UUID.
- That UUID is the `key` above. Anyone can create one, so plan-20's phone can
  spawn with no window open and no id collisions with `nextId`.
- `spawn` being idempotent makes StrictMode's double effect harmless.

`PANE_CAP` (`tree.ts:26`) moves to the daemon as well as the UI, so a client
cannot create a fifth session. The two stay in sync through a test.

**Boot (`App.tsx:35`).** `resetSessions` goes. After the layout is parsed,
reconcile it against `list`:

| Layout pane | Daemon | Result |
|---|---|---|
| Key is live | Yes | Attach; snapshot restores it. |
| Key is a tombstone, code 0 | Yes | Close the pane, `forget`. |
| Key is a tombstone, other code | Yes | Show `process exited (code N)`. |
| Key is unknown | No | Spawn fresh in the saved cwd, as today. |
| No pane for it | Live session | Add a pane with that key. |

The last row applies even with `restoreLayout` off. A running session is
never hidden.

**Pane lifetime.** `TerminalPane`'s cleanup at line 216 kills the session on
unmount, and at line 184 on a disposed spawn. Those become `detach`. A kill
now comes from one place: a layout store subscriber that sends `kill` for any
key that was in the previous tree and is not in the next one. So every path
that removes a pane is covered, and no unmount can end an agent.
`TerminalPane` calls `attach` or `spawn` depending on what reconciliation
found, and the `pendingInput` queue stays for the same reason as today.

The first bytes of an attach include the snapshot, so `spawnPty` keeps its
`OnData` shape. xterm is sized to the attach reply before the snapshot is
written.

## 6. Connecting and starting

**Pipe:** `\\.\pipe\veal-<user SID>` (`veal-dev-<SID>` in debug builds, so a
dev window never attaches to the installed daemon's sessions). The daemon
creates it with `first_pipe_instance`, `reject_remote_clients` and a DACL for
the current user only. The client checks that the pipe's server process is
owned by the same user before sending anything, because anything written to
this pipe runs as a shell. Transport is tokio's named pipes; tokio 1.53 is
already in the lock file through Tauri. PTY threads stay blocking.

**Starting:** the app connects, and on failure launches `veal-daemon.exe`
from next to `veal.exe`, then retries for up to 2 s. Two racing launches are
fine: the loser fails `first_pipe_instance` and exits. Launch with
`CREATE_BREAKAWAY_FROM_JOB` so the daemon outlives the app, and use
`windows_subsystem = "windows"`, the same conditions ConPTY runs under today.

**Exiting:** after 30 s with no sessions and no clients. A reboot or logoff
ends everything, and the next launch respawns from the layout, as today.

**Daemon death:** the app sees the pipe close. Each pane shows `background
service stopped` with a Restart button that respawns in the saved cwd. It
does not restart silently, because the agents' work state is gone and the
user should know.

## 7. The bridge in the app (`src-tauri/src/lib.rs`, `core/client.rs`)

A WebView cannot open a pipe, so the Tauri process connects. The `pty_*`
commands keep their shape and forward to the client. `pty-data`, `pty_read`
and `pty.ts`'s drain loop stay as they are, with `Shared` reused per pane on
the app side. The payload test keeps reading names from `lib.rs`.

One pipe carries every pane, so the read loop must never block on one pane's
full outbox. That would stall all of them. On overflow it discards that pane's
bytes and asks for a snapshot once JS has drained. A snapshot starts with
`ESC c` and is just bytes to `term.write`, which keeps `pty.ts` nearly
unchanged (subject to G5).

Removed: `pty_reset`, the `Destroyed` kill at `lib.rs:421`, and
`resetSessions`.

## 8. Closing, the tray and updates

**Close guard.** `CloseGuard.tsx` stays in charge of the decision. Unsaved
files still block as today. For agents, "Quitting ends these sessions"
becomes three choices:

- **Keep running** (default, focused): detach, close the window.
- **Stop and quit**: `shutdown`, close the window.
- **Cancel**.

With no agents running, closing behaves as today and sends `shutdown`, so idle
shells do not pile up unseen. A new setting, `closeWithAgents`
(`ask` / `keep` / `stop`), remembers the choice.

**Tray.** The daemon hosts it, since it is the process still alive after the
window closes. `tray-icon` runs on its own thread with a Win32 message loop.
Tooltip `Veal: 2 agents running`. Menu: Open Veal, Stop everything and quit.
Stop asks first if agents are running. The icon only shows while the daemon
has sessions and no window attached. A tray failure is logged and never takes
down the sessions. The tray is how a user learns that something is still
running, so it ships with Keep running, not after.

**Updates.** The installer cannot replace a running `veal-daemon.exe`, and
replacing the daemon ends its shells.

- `UpdateCard.atStake` keeps its warning. Agents are lost on update, the same
  as today.
- `install()`: `flushAll`, then `shutdown`, wait for the daemon to exit, then
  `update.install()`.
- An NSIS `installerHooks` pre-install hook stops the daemon by image name, as
  a backstop for a manually run installer. Uninstall gets the same hook.
- Bundle with `externalBin`. It requires
  `binaries/veal-daemon-x86_64-pc-windows-msvc.exe` to exist at build time,
  so `beforeBuildCommand` and `beforeDevCommand` build and copy it first via
  a new `scripts/build-daemon.mjs`. `release.mjs` itself should not need to
  change; confirm with `--dry-run`.

**Version mismatch.** `hello` carries a protocol number. Installs normally
stay in step because of `shutdown`, so a mismatch means something went wrong.
The app says so: `N sessions are running on an older background service`,
with Restart (ends them) or Quit. In debug builds, `build` is checked too, and
a stale daemon with no sessions is replaced automatically.

Deferred: keep the daemon alive across app updates by running it from a
versioned copy under `%LOCALAPPDATA%`. This would let an update spare running
agents, but it brings real version skew. Worth doing once the protocol has
settled, not in the first version.

## Dependencies

| Crate | Where | Why |
|---|---|---|
| `alacritty_terminal` 0.26 (subject to section 0) | core | Screen state |
| `tokio` (named pipes, already in the lock file) | core, daemon | Transport |
| `tray-icon` (0.24 already in the lock file) | daemon | Tray |
| `windows-sys` extra features | core | SID, `CreateEnvironmentBlock`, pipe owner |

`tauri` keeps its current features. One new JS dependency:
`@xterm/addon-unicode11` 0.9.0, tested with xterm 6.0.

## Build order

Each step leaves the app working, and tests pass after each one. **Do not
publish a release between 5 and 7:** the updater and close behaviour are only
right once all three are in.

1. **Gate** (section 0). Write the results at the top of this file.
2. **Crates** (section 1). `veal-core` only: move files, no behaviour change.
   `pnpm tauri build` and `release.mjs --dry-run` stay green.
3. **Sessions in-process** (sections 2 and 3). Emulator, fan-out and overflow
   behind the same commands, still inside `veal.exe`, with `panic = "unwind"` and
   `catch_unwind` arriving together. Move the serializer from `scripts/gate`.
   Re-run plan-15, and repeat G1 on a long session with tool use.
4. **Reattach in-process** (section 5). UUID keys, reconcile, kill via
   subscriber, `pty_reset` gone. F5 now restores panes instead of killing
   them. This is shippable on its own and is the best early test of restore.
5. **The daemon** (sections 4, 6, 7). The `veal-daemon` crate and its file logger,
   protocol, pipe, the bridge, the process.
6. **Lifecycle** (section 8): close guard, tray, setting.
7. **Packaging and updates** (section 8): `externalBin`, hooks, `shutdown`
   before install. A real update from a test version to a newer one.
8. **Acceptance** below, on the release build, then bump the version.

## Step 5 tasks

Tasks are in build order. Until 5.9, the app keeps using the in-process
`PtyRegistry`, so the app works after every task and the switch is one commit.
`PtyRegistry` and `Session` move into the daemon as they are. The daemon is a
listener in front of them, not a rewrite.

**Decisions this breakdown makes against sections 4–7** (change them here if wrong):

- **D1. Requests carry an `id`, and replies echo it.** A `spawn` resolves the
  proxy, which can block for up to 400 ms (`resolve_proxy` in `lib.rs`), so the
  daemon runs it off the connection's read loop. That means replies can arrive out
  of order. Events (`output`, `exit`, `agent`, …) have no `id`.
- **D2. Add a `resync { key }` message.** Section 7's "asks for a snapshot once JS
  has drained" has no message in section 4's table. `resync` drops that client's
  queue for the session, and the next `output` starts with a snapshot. That is the
  same path `Session::take` already uses past `OUTBOX_CAP`.
- **D3. The daemon owns agent detection.** `agents::start` runs in the daemon,
  `agent { key, agent }` goes to every client, and the `attach` reply carries
  `Entry.agent`, as `Opened` does today.
- **D4. Until step 6, closing the window sends `shutdown`.** It replaces the
  `kill_all` on `Destroyed` (`lib.rs:454`), so closing still ends every shell, as
  it does today. Without a tray yet, a kept daemon would be invisible.
  `on_page_load(Started)` becomes "detach this connection's clients".
- **D5. Pull the dev half of `scripts/build-daemon.mjs` into step 5.**
  `tauri dev` builds only the `veal` package, so `beforeDevCommand` must build
  `veal-daemon` too, or no dev window can start one. `externalBin` and the
  installer stay in step 7, so a step 5 installer has no daemon. That is one more
  reason not to publish between steps 5 and 7.
- **D6. Dev runs a copy of the daemon.** A running `veal-daemon.exe` locks its own
  file, so the next `cargo build` of it fails. In debug builds, the app copies the
  exe to `target\debug\veal-daemon-<build>.exe` and launches that copy. The
  `build` check in `hello` (section 8) then replaces a stale idle daemon. Release
  builds launch the file next to `veal.exe` directly.

**5.1 Logger** (`core/log.rs`). A small file logger written for Veal, with no new
crate: `%LOCALAPPDATA%\com.veal.app\logs\daemon.log`, rotated to `.1` at 1 MB, and
never panics. The two `eprintln!`s (`pty.rs:420`, `agents.rs:207`/`330`) go
through it. Test: rotation, and that a write to a missing folder is a no-op.

**5.2 Protocol** (`core/protocol.rs`, pure). The frame codec, plus
`ClientMsg`/`DaemonMsg` enums with serde. `PROTOCOL` is a const, and `MAX_FRAME`
limits allocation from a bad length. Output and snapshot frames are binary:
`kind`, `u32 session`, bytes. Session numbers are per connection and map to keys,
which saves sending a UUID per chunk. Tests: round trips, an oversized length, a
truncated frame, an unknown kind, all errors with no panic.

**5.3 Spawn environment** (`pty.rs` `start`). `CreateEnvironmentBlock` with the
process token and `bInherit = FALSE`, then `env_clear()` and set each variable.
After that come `TERM`, `VEAL_PANE`, the proxy, and the spawn's optional `env`
overlay. Debug builds send their whole environment as the overlay, so a dev
shell's variables still reach panes. Behind a `cfg(windows)` function, falling
back to the inherited environment if the call fails (logged). Test: a variable
set only in the test process is missing with no overlay and present with one.
Needs `windows-sys` `Win32_System_Environment`, `Win32_System_Threading`,
`Win32_Security`.

**5.4 Pipe name and security** (`core/pipe.rs`).
- The current user's SID as a string: `OpenProcessToken`, `TokenUser`,
  `ConvertSidToStringSidW`.
- The pipe name `veal-<SID>`, or `veal-dev-<SID>` with `cfg(debug_assertions)`.
  `VEAL_PIPE` overrides it, for tests.
- Server: tokio `ServerOptions` with `first_pipe_instance(true)` for the first
  instance only, `reject_remote_clients(true)`, and a DACL that grants only the
  user's SID, via `create_with_security_attributes_raw`.
- Client: `GetNamedPipeServerProcessId`, then that process's token user must
  equal ours, before any bytes are sent.
- Tests: the name format, the owner check passing on our own pipe. The
  other-user case is acceptance 14, by hand.

**5.5 Daemon crate** (`src-tauri/daemon`, `veal-daemon`, added to `[workspace]`).
`windows_subsystem = "windows"`, a tokio runtime, and one `Arc<PtyRegistry>`.
- **Accept loop.** It keeps a pending server instance ready at all times. If
  `first_pipe_instance` fails, another daemon won: log and exit 0.
- **Per connection.** It reads `hello` first and checks the protocol. Then it
  dispatches: `list`, `spawn` (on a blocking task, as in D1), `attach`, `detach`,
  `write`, `resize`, `kill`, `forget`, `theme`, `resync`, `shutdown`. One writer
  task owns the pipe's write half, fed by an unbounded channel of frames. A
  session's `Notify` sends a "take from (session, client)" request to that
  channel, so the PTY reader never touches the pipe.
- **When a connection drops,** all its clients are detached, which is today's
  `detach_all` scoped to one connection. Owed answers are sent as before.
- **Broadcasts.** `exit`, `agent`, `session_added` and `session_removed` go to
  every connection.
- **`shutdown`** runs `kill_all`, waits for the waiters with a cap, then exits.
- **Idle exit.** A 1 s tick exits after 30 s with no live sessions and no
  connections.
- **Panics.** A panic in a connection task ends that connection only, and is
  logged.
- **Tests:** unit tests for the dispatch against a `PtyRegistry` with the pipe
  left out.

**5.6 Client** (`core/client.rs`). Connect, check the owner, send `hello`. Pending
requests are a map from `id` to oneshot. One reader task decodes frames and
routes them: replies to their oneshot, `output` to a per-session sink callback,
and events to an event callback. A closed pipe fails every pending request and
raises one `Disconnected` event.

**5.7 Starting the daemon** (`core/client.rs`, `launch`). Try to connect. On
`NotFound`, launch the daemon:
- Release builds use the exe next to `current_exe()`; debug builds use the copy
  from D6.
- Flags: `CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW | DETACHED_PROCESS`. If
  that fails with access denied, retry without breakaway and log it (a known
  unknown).
- Retry connecting every 50 ms for up to 2 s.
- `hello` mismatch, debug builds: if the stale daemon has no sessions, send
  `shutdown`, wait for it to exit, and relaunch. Release builds: return a typed
  error. The UI for it waits for step 6. Until then the pane shows the error text.

**5.8 Integration tests** (`daemon/tests/`). A real daemon on a unique `VEAL_PIPE`,
driven by `client.rs`:
- Spawn, detach, reconnect, attach, and compare screens.
- Idempotent spawn by key, and the cap.
- `exit 3` while detached is a tombstone on reattach.
- `resync` after a forced overflow.
- Two connections: one stalled, one fast, and the child not slowed.
- `shutdown`, and idle exit with the timeout shortened through an env var.
- A bad frame from a client drops that connection only.
- Existing `pty.rs` and `session.rs` tests are unchanged.

**5.9 The bridge** (`src-tauri/src/lib.rs`). This is the switch.
- `run()` connects, or launches the daemon (5.7), in `setup` and manages a
  `Bridge`. The `pty_*` commands keep their names and arguments, and forward.
  `PtyRegistry` and `agents::start` leave the app.
- The app keeps its own `u32` id per key, so `pty.ts`'s id-keyed maps, `pty-data`,
  `pty-exit` and `pty-agent` stay as they are. Each pane gets an app-side
  `Shared` outbox, capped at `OUTBOX_CAP`.
- The read loop never blocks. On overflow it clears that pane's outbox and sends
  `resync` once JS has drained it. The snapshot then starts with `ESC c`, which
  needs no change in `pty.ts`.
- `on_page_load(Started)` sends `detach` for this window's clients. `Destroyed`
  sends `shutdown` (D4).
- `Disconnected` emits `pty-daemon-lost`.
- Remove: the `kill_all` on `Destroyed`, and the app's `veal-core::pty` import
  except types.
- Payload test: still reads names from `lib.rs`.

**5.10 UI** (`pty.ts`, `TerminalPane.tsx`).
- On `pty-daemon-lost`, every pane shows `background service stopped` and a
  Restart button. Restart reconnects or relaunches, then `pty_open`s the same key
  in the saved cwd.
- The theme is reported with `pty_theme` on open and on mode change
  (`TerminalPane.tsx:237`), so OSC 10/11 are right while headless.
- Tests: the lost state, and that Restart reopens with the same key.

**5.11 Dev wiring** (D5, D6). `scripts/build-daemon.mjs`, dev half only: run
`cargo build -p veal-daemon` in `beforeDevCommand` before `pnpm dev`, in the
matching profile. Document in it that step 7 adds the `externalBin` copy.

**5.12 Live checks** on `cargo build --release --workspace`, with both exes in
`target\release`, over CDP as in step 4:
- **Step 4's table again:** every row.
- **Acceptance 3 (F5), 4 (end `veal.exe` mid-run, relaunch restores), 5 (end the
  daemon), 7, 10 (plan-15 floods across one pipe), 11 (`PATH`), 13 (proxy),
  15 (idle cost), 16 (sleep).** These are the parts of acceptance that need only a
  daemon.
- **Known unknowns:** whether a console window flashes when a GUI-started daemon
  spawns ConPTY, whether breakaway is refused when launched from Windows Terminal
  and from an IDE terminal, and the `veal` launchers that `pty.rs:228` mentions.
- Write the results at the top of this file, like steps 2–4.

## Tests

Rust:

- `session.rs`: snapshot then output has no gap or overlap with concurrent
  writes. Overflow drops and resyncs. One stalled client does not slow another
  client or the child. Tombstones. An emulator panic leaves the shell alive.
- Queries: answered once with a responder, once without, none from a
  non-responder.
- `protocol.rs`: round trips, oversized and truncated frames are errors and
  do not panic, wrong protocol number.
- Integration: a real daemon on a unique pipe name, driven by `client.rs`.
  Spawn, detach, reconnect, attach, compare screen. Idempotent spawn, the cap,
  shutdown, idle exit. A pipe owned by another identity is refused where this
  can be set up.
- Existing `pty.rs` and `agents.rs` tests move with the code.

TS:

- `parseNode` adds a key to old snapshots and keeps existing keys.
- The reconcile table, one test per row.
- The kill subscriber: closing a pane kills, remounting does not, boot's
  `setState` does not.
- `PANE_CAP` agrees with the Rust value (`?raw`, as the payload test does).

## Acceptance

Run on the release build, with a real `claude` session unless stated.

1. Start an agent on a long task, close the window, choose Keep running. The
   tray shows it. Reopen 5 minutes later: same pane, same screen, scrollback
   intact, the red badge and the sidebar dot are correct at once, and the
   agent finished the work while the window was closed.
2. Type into the reattached agent: prompt, Shift+Enter, paste text, paste an
   image, Ctrl+C. All work.
3. F5 with an agent mid-run: it keeps running with no junk in its input.
4. End `veal.exe` in Task Manager mid-run: the agent keeps running, and
   relaunching restores it.
5. End `veal-daemon.exe` in Task Manager: panes show the stopped message and
   Restart gives a working shell in the same cwd.
6. Close with idle shells only: no prompt, no tray icon, and the daemon is gone
   within 30 s.
7. `exit` in a pane while attached closes it. `exit` while detached closes it
   on reopen. `exit 3` while detached shows `process exited (code 3)`.
8. Spawn a pane, then close the window within a second, choosing Keep running:
   the shell still starts and is usable on reopen. (The ConPTY stall.)
9. Close the window while a pane is mid-spawn on a slow machine, repeated 20
   times: no session hangs.
10. Re-run plan-15's flood list. Nothing stalls, every pane answers `echo` after
    Ctrl+C. A window frozen with the debugger during a flood does not slow the
    child; after it unfreezes, the screen is correct.
11. Install a tool that changes `PATH`, open a new pane without restarting the
    daemon: the tool is found.
12. Update from a test version with an agent running: warned first, the daemon
    is stopped, install succeeds, the new version starts its own daemon. Also
    uninstall: no process or locked file remains.
13. Proxy: plan-18's A1 and A10 still pass. New panes get the same variables as
    the Settings row.
14. From another local user account: cannot connect to the pipe.
15. Idle cost: with four panes and nothing running, the daemon is under 1% of a
    core. Report memory with 5000 lines in each.
16. Sleep and resume with an agent running: the session and the pipe are both
    still working. (Sleep pausing the agent is expected.)

## Known unknowns

- Whether restore holds on much longer agent sessions than the one three-turn run
  step 3 checked, which already turned up one bug.
- Whether a daemon started from a GUI app behaves the same under ConPTY as
  panes do now, including no console windows flashing up. Checked in step 5.
- Antivirus and SmartScreen. Veal's installer has an updater signature, and I
  found no Authenticode step in `release.mjs`. An unsigned background process
  that launches shells is more likely to be flagged than a windowed app. Test
  on a clean machine before release; signing may become a real need.
- Whether `CREATE_BREAKAWAY_FROM_JOB` fails when the app's parent job forbids
  it, for example when launched from some terminals. It needs a fallback to
  launching without the flag.
- The `veal` launchers that `pty.rs:228` refers to are not in this repo.
  Confirm they do not assume a shell dies with the window.
- Tauri's NSIS details are from memory and not yet checked against the current
  version: the `installerHooks` setting, and how its running-app check treats
  a sidecar. Confirm both at the start of step 7.

## Rejected

- **A Windows service.** Runs in session 0, away from the user's desktop,
  profile and tokens, and needs elevation to install. Shells must be the user's.
- **Keep `veal.exe` alive with a hidden window and a tray.** Far less work, and
  it does give "agents survive closing the window". It does not survive a UI
  crash or a page reload unless reattach is also built, keeps a WebView running
  in the background, and leaves plan-20 inside the UI process. It is the fallback
  if the gate fails or the daemon's cost looks wrong after step 4.
- **tmux or similar through WSL.** Adds WSL as a requirement, and panes would no
  longer be native PowerShell.
- **Moving Files, Tasks or the watcher now.** Nothing needs them without a window
  until plan-20, and each is a separate migration.

## Revision log

- 2026-11-07: first version. The daemon has no database and keeps the project
  list out, which is narrower than first discussed: nothing headless spawns
  until plan-20.
- 2026-11-07: gate run. Results at the top; snapshot rules and the Unicode 11
  addon added to section 2; the G6 reflow claim corrected.
- 2026-11-07: step 2 built. Panic setting moved to step 3, logger and daemon crate
  to step 5.
- 2026-11-07: step 2's dry run and pane launch passed with Veal closed.
- 2026-11-08: step 3 built. Repeating G1 found the stale-wrap bug, now fixed. Fresh
  environment and theme reporting moved to step 5.
- 2026-11-08: step 4 built. The live run found tombstones dropped too early and a
  shell stuck on its opening query after a reload; sections 2 and 3 changed to match.
- 2026-11-09: step 5 broken into tasks (section "Step 5 tasks"), with request ids,
  `resync`, close-sends-`shutdown` until step 6, and the dev daemon build pulled in.
