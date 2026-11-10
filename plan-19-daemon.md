# Veal — plan for step 20: Shells live in a background daemon

Status: build order 1 (the gate) RUN 2026-11-07: passed, with the conditions
below. Step 2 BUILT and checked 2026-11-07. Step 3 BUILT 2026-11-08, with two
plan-15 checks left for a person (below). Step 4 BUILT 2026-11-08. Step 5 BUILT
2026-11-09, with three checks left for a person (below). Step 6 BUILT 2026-11-09,
with the real-agent and tray-click checks left for a person (below). Step 7 BUILT
2026-11-10, with a clean-machine check left for a person (below). Step 8: tasks
8.1–8.5 DONE 2026-11-10. 8.6 waits on the person checks in `plan-19-checks.md`,
the PR review, and the user's word to release.

Results for 8: the acceptance tools are in `scripts/acceptance/`, now including
`rows.ps1`, which runs every scriptable row against the installed test build (8.1).
The version is 0.1.3 in all five places, and `src/version.test.ts` keeps them equal
(8.2). `cargo test --workspace` 172 (+3 ignored), `pnpm test` 246.

8.3, on candidate 0.1.92 (test identity, signed, installed with `/S`, the
`claude.exe` stand-in for the agent). The first candidate failed row 5. The fix
went in, the candidate was rebuilt, and every row ran again on the rebuild:

| Row | Result on the rebuilt candidate |
|---|---|
| 3. F5 twice with an agent | Same agent PID and daemon, badge `claude`, nothing typed into it |
| 4. End `veal.exe` | Agent and daemon lived; the relaunch reattached to the same daemon, badge `claude` |
| 5. End `veal-daemon.exe` | "background service stopped"; Restart gave a new daemon, a shell in the same cwd, and the badge back to `shell` |
| 6. Close with idle shells | No dialog, no tray; app and daemon gone in 0.7–0.8 s, also with four panes of 5000 lines (6b) |
| 7. `exit` | Attached: the pane closes. While the app was ended: dropped on relaunch. `exit 3` the same way: `process exited (code 3)` |
| 8. New terminal, close at once, Keep running | The new pane answered on reopen; same daemon, agent kept |
| 9. That, 20 times | 20/20 closed and answered, one daemon throughout, agent kept |
| 10. Flood, four panes, 60 s | About 2,800 lines/s per pane, none stopped (e.g. 56k/110k/164k lines at 20/40/60 s); all answered after Ctrl+C. Page frozen in the debugger for 30 s: the child finished 60,000 lines in ~14 s, and on resume the screen ended `line 60000`, `END-MARK`, prompt |
| 12. Closing and tray | Ask: dialog names the agent, Keep running has focus; Keep: agent kept, tray "Veal: 1 agent running", hidden on reopen. Saved Stop: no dialog, all gone in 1.7 s. Saved Keep: no dialog (rows 8–9) |
| 13. Proxy (Veee on) | New pane `https_proxy=http://127.0.0.1:15236`, `curl.exe -I https://api.anthropic.com` answered through it (HTTP 404 at the root), Settings row "Follow system · http://127.0.0.1:15236" |
| 15. Idle cost | Four idle panes with 5000 lines each: daemon 0.44–0.78% of one core over 60 s; private 46.7–47.2 MB daemon, 8.7–9.5 MB `veal.exe` (WebView2 ~433 MB) |

Row 11, run separately with the user's go-ahead: the user `Path` was saved, a
folder holding `vealtool.cmd` was added, and `WM_SETTINGCHANGE` was broadcast. A
pane opened before the change could not find it. A new pane ran it and `where`
found it, with the same daemon PID. The `Path` was then restored byte for byte
(still `REG_EXPAND_SZ`). Rows 1, 2, 14 and 16 are in `plan-19-checks.md`.

Found and fixed: after the daemon was ended and Restart pressed, the pane kept its
old shell's `claude` badge and sidebar dot. `onAgent` reported only a running
agent on attach, so nothing cleared the layout's entry. It now reports null too
(a97c2b4, with a test).

8.4, upgrade from v0.1.2: the `v0.1.2` tag built under the test identity as 0.1.89
(no daemon), with a project pinned and two panes, one counting. Its card offered
0.1.92 from the local feed. Restart to update ran the installer with
`/P /UPDATE` and no prompt, and 0.1.92 was running with a daemon from the install
folder about 7 s after the click. Both panes came back from the old layout with new
session keys, in their saved folder. The counting shell had ended with the old app,
as v0.1.2's shells do. Close with Keep running (agent in a pane), then reopen: both
panes back on the same daemon, and they answer. Uninstall left no process or folder.

Notes: the daemon's build string is now `release-0.1.3`. The test build's logs
go to `%LOCALAPPDATA%\com.veal.app\logs`, the same folder as the real Veal's,
because `core/src/log.rs` names it directly. The rest of the test identity is
separate. A window close was once seen not to happen (the app was still open when
the installer later ended it). That was after a manual debugger pause, and it did
not happen again in three full runs.

Results for 7: `pnpm bundle` makes an installer that carries `veal-daemon.exe`
(F1). The installer and uninstaller stop the daemon after asking about Veal itself
(F2–F4). An update stops it before installing, and a daemon of another protocol
gets a Restart/Quit prompt instead of pane errors (tasks 7.1–7.5, plus one fix).
`cargo test --workspace` 172 (+3 ignored): 50 app, 97 core, 12 daemon, 13 daemon
integration (new: `--stop`, mismatch then replace). `pnpm test` 244. Clippy has the
same single warning. Installer 5.48 MB with the daemon (was 5.02 without it).
`release.mjs --dry-run` signs it and passes its new check.

Live, under the test identity (F5), with installers 0.1.90 and 0.1.91 signed with
the real key and served from `127.0.0.1:8765`. Installer dialogs were driven
through UI Automation (`BM_CLICK`), the app over CDP, and the agent was the
`claude.exe` stand-in from step 6:

| Check | Result |
|---|---|
| 1. Install 0.1.90 (`/S`) | Folder has `veal.exe` and `veal-daemon.exe`; the daemon runs from it; a pane runs a command |
| 2. Acceptance 12, update with an agent running | Card: "Updating stops Veal's background service: 1 running agent will end." Update anyway: daemon and agent gone within 2 s, installer ran with no prompt, 0.1.91 relaunched with a new daemon (16236, was 18540) |
| 3. Manual install over a running app with an agent | "Veal Test is running!" first. Cancel aborted, with the agent, daemon and app untouched. Again with OK: `veal.exe` ended, daemon kept, then "Installing ends every shell…"; OK stopped it through `--stop` (log `[stop] … Stopped`) and the install completed |
| 4. Manual install with only the daemon (Keep running) | Straight to the shells prompt; OK stopped daemon and agent; install completed |
| 5. Acceptance 12, uninstall with the daemon running | "Uninstalling ends every shell…"; OK: no `veal-daemon.exe` or `veal.exe` left, the install folder gone |
| 6. Mismatch (debug, a `--protocol 99` daemon with one session) | Prompt "1 session is running on an older version…"; Restart ended it and the panes run on a new daemon |
| 7. `release.mjs --dry-run` | Signed, and `installer.nsi` names `veal-daemon.exe` |

Found while checking, fixed: the mismatch prompt's Restart lost focus to a pane that
mounted at the same moment, and the pane error said "1 sessions are".

Notes: the daemon reports its build as `release-0.1.2`, the daemon crate's version,
not the app's, so an update's daemon has the same build string; only the PID tells
them apart in the log. 7.5's modal has no component test: the project has no DOM test
setup, so the commands it calls are tested against `lib.rs` instead. 7.3 and 7.5
share a commit for the daemon and client, and 7.4 and 7.5 one for the app commands,
since they change the same lines. After a Rust edit in `tauri dev`, the daemon
went down with the old app, since this sandboxed shell's job forbids breakaway (as
in step 5); from a normal terminal it would survive.

Left for a person: SmartScreen and antivirus on a clean machine (known unknown); the
installer's look (only its text was read).

Results for 6: closing with agents running asks Keep running (default), Stop and
quit, or Cancel, and `closeWithAgents` (Settings, or the dialog's "Don't ask
again") skips the dialog. Keep running leaves the shells in the daemon, which shows
a tray icon while no window is attached (tasks 6.1–6.5, one commit each, plus three
fixes). `cargo test --workspace` 170 (+3 ignored): 50 app, 97 core, 12 daemon, 11
daemon integration. `pnpm test` 240. Clippy has the same single warning. Installer
5.02 MB, `veal-daemon.exe` 1.57 MB (was 1.35).

Live, on the release build over CDP. The agent was a stand-in, `ping.exe` copied to
`claude.exe`, which agent detection treats as `claude`:

| Check | Result |
|---|---|
| Acceptance 6: close with idle shells | No dialog, no tray; app and daemon gone in 0.3 s |
| Close with an agent, setting Ask | Dialog "Agents running", the agent listed, Keep running focused |
| Keep running | App gone, daemon and agent kept (same PID); log `[tray] shown: Veal: 1 agent running` |
| Reopen (what the tray's Open Veal launches) | Same daemon, agent PID unchanged, CLAUDE badge and sidebar dot at once, screen restored; `[tray] hidden` |
| Stop and quit, "Don't ask again" ticked | Everything gone in 0.3 s; next close with an agent skipped the dialog and stopped all |
| Setting Keep running in Settings | Close with an agent skipped the dialog and kept everything |
| Acceptance 8: New terminal, then close at once | The new pane came back on reopen and ran a command |
| Acceptance 9: that, 20 times | All 20 closes went through; every pane ran a command afterwards |

Found while checking, all fixed: integration-test daemons put up tray icons (only
the user's own pipe gets a tray now); in a workspace build Tauri switches on
muda's `common-controls-v6`, and the daemon, with no manifest, failed to load
(`0xC0000139`), so `cargo test --workspace` failed while `-p veal-daemon` passed
(the daemon now embeds a Common Controls v6 manifest); the "Don't ask again"
checkbox sat on its own line.

Left for a person: acceptance 1 and 2 with a real `claude` on a long task (only the
stand-in was used, so no prompt, paste or Ctrl+C into a reattached agent); clicking
the tray icon itself (left click and Open Veal launch `veal.exe`; Stop everything
asks with a message box when agents run); seeing the icon and its tooltip in the
taskbar (only the log was checked); restarting Explorer while the icon shows
(`tray-icon` re-adds it on `TaskbarCreated`; not tried).

Results for 5: shells live in `veal-daemon.exe`, behind a user-only named pipe, and
the app is a client of it (tasks 5.1–5.11, one commit each). `cargo test --workspace`
167 (+3 ignored): 50 app, 97 core, 9 daemon, 11 daemon integration against a real
daemon. `pnpm test` 234. Clippy has the same single warning. Installer 5.02 MB
(no daemon in it yet, D5); `veal.exe` 15.3 MB, `veal-daemon.exe` 1.35 MB. The
updater signature step fails here for want of the private key, as expected.

Live, on the release build over CDP, typing through `Input.insertText` (a release
build has no `__veal` hook) and checking files the shells wrote and screenshots:

| Check | Result |
|---|---|
| Step 4's table: F5 twice with a child counting | Same shell PID, child never stopped (59→80→99), screen restored, Ctrl+C and typing work |
| Close the app (D4) | Window close sends `shutdown`: app, daemon and every shell gone within 4 s |
| A3: F5 twice with `claude` open | Input box empty, badge and sidebar dot right at once; a key and a double Ctrl+C reach it after; badge clears on exit |
| A4: end `veal.exe` mid-run | Child kept writing (44→116 while gone); relaunch attached to the same daemon and showed the ticks it missed |
| A5: end the daemon | All four panes show "background service stopped"; Restart gives four new shells in `~\veal` on a new daemon |
| A7 | `exit` attached closes the pane; `exit` while the app was ended: dropped on relaunch; `exit 3` while ended: `process exited (code 3)` on relaunch and on every reload |
| A10: four panes flooding 60 s | None stopped; UI timer lag p50 0.8 ms, p99 6.2 ms, max 36 ms; all four answered after Ctrl+C. `veal.exe` 8.2→9.5 MB private, daemon 13.5→46 MB |
| A10: page frozen in the debugger | A 60 000-line flood took 29 s rendered and 15 s with the page paused; screen right on resume (last line, END-MARK, prompt) |
| A10: closing a flooding pane / `exit` after Ctrl+C mid-flood | Shell gone within 2.5 s / pane closes |
| A11 | `setx` of a test variable (then deleted): a new pane sees it, old panes do not, same daemon PID |
| A13 | New pane's `http_proxy`, `https_proxy`, `no_proxy`, `NODE_USE_ENV_PROXY` equal to `proxy_resolve`'s row; `curl` through it reaches the API |
| A15: four idle panes, 5000 lines each | Daemon 0.91% of a core over 60 s, 47 MB private; app 0.31%, 9.9 MB |
| Console flash | Ten spawns: every conhost under the daemon has `MainWindowHandle` 0 |
| Breakaway | Launched from Explorer and from Windows Terminal: no fallback in the log. Closing the WT window left app and daemon running |
| Launchers | `veal.cmd` only `start`s the exe and exits its cmd unless `VEAL_PANE` is set: nothing assumes a shell dies with the window |

Left for a person: A16 (sleep and resume with an agent running), launching from VS
Code's terminal (not installed here), and watching the screen itself for a console
flash (only `MainWindowHandle` was checked). Launched from the sandboxed shell used
for these runs, the log shows "job forbids breakaway; daemon starts inside it", so the
fallback works too.

Found while building, all fixed with tests: the daemon inherited the test harness's
stdout (launch now uses `CreateProcessW` with no inherited handles); React's double
effect plus a detach of the newer attach left no current client (the newest remaining
attach is promoted, on both sides of the pipe); the first key typed after a reload was
swallowed by a second cursor report (the responder role goes back to the newest client
that asked for it); Ctrl+C did nothing in panes, because "ignore Ctrl+C" was inherited
down the launch chain (the daemon resets it at start, and `CREATE_NEW_PROCESS_GROUP`
is gone). Notes: the idle daemon's 0.91% is close to the 1% bar and is mostly the
agent scan every 3 s, which `veal.exe` ran before; `curl` with revocation checks fails
through the Veee proxy with or without Veal (`CRYPT_E_REVOCATION_OFFLINE`), so A13's
curl used `--ssl-no-revoke`.

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

- **D7. `spawn` attaches the caller.** It is today's `PtyRegistry::open`: it
  attaches to the key or spawns it, and the reply has the same shape as
  `attach`'s. ConPTY's opening query needs a responder attached before the shell
  starts (`FIRST_CLIENT`), so a separate `spawn` followed by `attach` would race
  it. `attach` stays for a client that must never spawn, which is plan-20's phone.
- **D8. A snapshot has its own frame kind.** `Taken` gains `snapshot: bool`, and
  the daemon sends `snapshot` instead of `output` when it is set. After a
  `resync`, the bridge drops `output` frames until a `snapshot` frame arrives, so
  bytes already on the pipe before the resync cannot land after the reset.
- **D9. Input has one writer thread per session in the daemon.** A write to a
  child that has stopped reading blocks (plan-12). On the connection's read loop
  that would stall every pane; on a thread pool it would reorder keystrokes. So
  each session gets a channel and a thread, made at spawn, which end with the
  session. Input is a binary frame, so a paste is not JSON-escaped.
- **D10. The registry id goes on the wire, and the app keeps its own ids.**
  `output`, `snapshot` and `input` frames carry the daemon's `u32` registry id,
  which saves sending a UUID per chunk. The app gives each pane its own id from a
  counter that never resets, so a restarted daemon's id 1 can never reach a JS
  map that still holds the old id 1.

Order: 5.1–5.4 do not depend on each other. 5.5 needs 5.1, 5.2 and 5.4. 5.6
needs 5.2 and 5.4. 5.7 needs 5.6. 5.8 needs 5.5–5.7. 5.11 has to land before 5.9,
or no dev window can start a daemon. 5.10 comes after 5.9. Each task is one
commit, and `cargo test --workspace`, `pnpm test` and clippy pass after each.

### 5.1 Logger (`core/src/log.rs`)

A small file logger written for Veal, with no new crate.

1. API: `log::init(name: &str)` sets the file once, through a `OnceLock<Mutex<File>>`.
   `vlog!(...)` formats the time, the thread name and the message. Before `init`,
   or after any I/O error, it is a no-op.
2. The path is `%LOCALAPPDATA%\com.veal.app\logs\<name>.log`, and the folder is
   created on `init`. When a write takes the file past 1 MB, rename it to
   `<name>.log.1`, replacing the old one, and reopen. Only two files ever exist.
3. Never panic: every `Result` is dropped, and a poisoned lock is used anyway.
4. Replace `pty.rs:420` and `agents.rs:207` and `:330` with `vlog!`, still only
   under `cfg(debug_assertions)` where they are now.
5. The app calls `log::init("app")` in `setup`, and the daemon calls
   `log::init("daemon")` first thing.

Tests, with a test-only `init_at(path)`: rotation at the limit leaves exactly
`.log` and `.log.1`, a missing or unwritable folder does nothing, and a write
before `init` does nothing.

Done when: no `eprintln!` is left in `core/` (grep), and the tests pass.

### 5.2 Protocol (`core/src/protocol.rs`)

Pure code with no I/O, so it is tested in isolation.

1. Frame: `u32 len` (LE, counting the kind byte and the payload), `u8 kind`, then
   the payload. `MAX_FRAME` is 16 MB. `decode(buf: &mut BytesMut-like Vec) ->
   Result<Option<Frame>, ProtoError>` takes one frame off the front, or returns
   `None` if more bytes are needed. `encode(&Frame) -> Vec<u8>`. A length over
   `MAX_FRAME` is an error before any allocation.
2. Kinds: `1` control (JSON), `2` output, `3` snapshot, `4` input. Kinds 2–4 are
   `u32 session` (LE) and then raw bytes.
3. `PROTOCOL: u32 = 1`.
4. `ClientMsg`, serde-tagged `{"t": "...", "id": n, ...}`:
   - `Hello { protocol, build, role }`
   - `List`
   - `Spawn { key, cwd, cols, rows, shell, proxy_mode, proxy_port, proxy_bypass, env: Option<Vec<(String,String)>> }`
   - `Attach { key, cols, rows, responder, controller }`
   - `Detach { key }`, `Resize { key, cols, rows }`, `Kill { key }`, `Forget { key }`, `Resync { key }`
   - `Theme { fg, bg }`, `Shutdown`

   `id` is on requests that get a reply (D1).
5. `DaemonMsg`:
   - `Reply { id, result: Result<ReplyBody, String> }`, where `ReplyBody` is
     `Hello { protocol, build, sessions }`, `List(Vec<SessionInfo>)`,
     `Opened { session, exited, code, agent }` or `Ok`.
   - Events: `Exit { key, code }`, `Agent { key, agent }`,
     `SessionAdded { key }`, `SessionRemoved { key }`.
6. Move `SessionInfo` from `pty.rs` into a type both sides use; `pty.rs`
   re-exports it so the app's import keeps compiling.

Tests:
- Every message round-trips.
- Feeding a frame one byte at a time gives `None` until the frame is complete,
  then the frame.
- Two frames in one buffer come out in order.
- A length of `MAX_FRAME + 1` is an error, and so are a length of 0, an unknown
  kind, a binary frame shorter than 4 bytes, and invalid JSON.
- A fuzz-style loop over random bytes never panics.

Done when: the module compiles with no I/O imports, and the tests pass.

### 5.3 Spawn environment (`core/src/pty.rs`, new `core/src/userenv.rs`)

1. `userenv::fresh() -> Option<Vec<(OsString, OsString)>>`:
   - Call `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE)`,
     then `CreateEnvironmentBlock(&mut block, token, FALSE)`.
   - Walk the block, which is UTF-16 entries ending in `\0` with a final `\0\0`.
     Split each entry on the first `=` after position 0, so the `=C:` style
     entries survive.
   - Call `DestroyEnvironmentBlock` and close the token.
   - Return `None` on any failure. It is `cfg(windows)`; other platforms return
     `None`.
2. In `start`, before `TERM`: if `fresh()` is `Some(vars)`, call
   `cmd.env_clear()` and set each var. If it is `None`, log it and keep inheriting.
3. `SpawnOptions` gains `env_overlay: Option<Vec<(String, String)>>`, applied after
   the fresh block and before `TERM`, `VEAL_PANE` and the proxy, so those always
   win. The app fills it in debug builds with `std::env::vars()`, and with `None`
   in release builds. This happens now, in `pty_open`, so dev panes do not lose
   the dev shell's variables before the switch.
4. `windows-sys` features for core: `Win32_System_Environment`,
   `Win32_System_Threading`, `Win32_Security`.

Tests, ignored off Windows:
- `fresh()` contains `PATH` and `USERPROFILE`, and not a variable the test set with
  `std::env::set_var` just before.
- A spawn with no overlay prints `%VEAL_TEST_X%` literally, so the variable is
  unset. With an overlay setting it, the value is printed.
- The existing proxy spawn test still sees its variables.

Done when: the tests pass, and in a dev run a pane still sees a variable exported
in the terminal that ran `pnpm tauri dev`.

### 5.4 Pipe name and security (`core/src/pipe.rs`)

1. `user_sid() -> io::Result<String>`: `OpenProcessToken`, then
   `GetTokenInformation(TokenUser)` called twice (once for the size), then
   `ConvertSidToStringSidW`, then `LocalFree`.
2. `pipe_name() -> String`: `VEAL_PIPE` if it is set, otherwise
   `\\.\pipe\veal-<sid>`, or `veal-dev-<sid>` under `cfg(debug_assertions)`.
3. `server(name, first: bool) -> io::Result<NamedPipeServer>`:
   - Build the security descriptor from SDDL `D:P(A;;GA;;;<sid>)` with
     `ConvertStringSecurityDescriptorToSecurityDescriptorW`, and wrap it in a
     `SECURITY_ATTRIBUTES`.
   - Pass it to `ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true)
     .create_with_security_attributes_raw(name, &mut sa)`, then free the
     descriptor.
   - The "first" error that matters is `ERROR_ACCESS_DENIED`, which is what a
     second daemon gets. Map it to `io::ErrorKind::AddrInUse`.
4. `check_server_owner(handle) -> io::Result<()>`:
   - `GetNamedPipeServerProcessId`, then
     `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, then `OpenProcessToken`,
     then `TokenUser`.
   - `EqualSid` against our own SID. Anything else is a `PermissionDenied`
     error, sent before the client has written anything.
5. Features: `Win32_Security_Authorization`, `Win32_System_Pipes`,
   `Win32_Foundation`. tokio in core with `net`, `io-util`, `rt`, `sync`, `time`
   and `macros`. It is already in the lock file at 1.53.1, so check that
   `Cargo.lock` gains features, not versions.

Tests, on a unique `VEAL_PIPE`:
- `user_sid()` starts with `S-1-5-21-`.
- `pipe_name()` uses the override.
- A second `server(name, true)` while the first is open fails with
  `AddrInUse`.
- `check_server_owner` passes on our own server.

Done when: the tests pass. The other-user check waits for acceptance 14.

### 5.5 The daemon crate (`src-tauri/daemon`)

First, core changes, kept in a commit of their own:
- `attach_key(key, cols, rows, responder, controller, notify)`. Only a controller
  resizes. The app's current call passes `true, true`.
- `PtyRegistry::open` takes the same two flags.
- `Session::resync(client)` sets the existing `resync` flag. `Taken` gains
  `snapshot: bool`, and `take_from` returns `Taken` instead of `Vec<u8>`.
- `key_of(id)` lets the agent sink map ids to keys.
- `set_theme_all(fg, bg)` stores the theme and applies it to every session and
  to each new one in `start`.
- `write` takes its writer from a per-session input thread (D9). `start`
  creates a `mpsc::Sender<Vec<u8>>` and a `pty-{id}-input` thread that does the
  `write_all` and `flush`, and `write` only sends to the channel. This also
  helps the in-process app, and the existing write tests cover it.

Then the crate:
1. `daemon/Cargo.toml`: a bin named `veal-daemon`, with `veal-core`, tokio
   (`rt-multi-thread`) and `serde_json`. Add `"daemon"` to the workspace members.
   `#![windows_subsystem = "windows"]` in `main.rs`.
2. `main.rs`:
   - Call `log::init("daemon")` and install a panic hook that logs.
   - Parse `--build <id>` (5.7). Build a `State`, holding the
     `Arc<PtyRegistry>`, the connections
     (`Mutex<HashMap<ConnId, mpsc::UnboundedSender<Out>>>`), the last theme and
     the build id.
   - Start `agents::start(registry, sink)`. The sink maps the id to a key and
     broadcasts `Agent`.
   - Create the first server with `first = true`. On `AddrInUse`, log and exit 0.
   - Accept loop: `server.connect().await`, then create the next instance
     *before* spawning the connection task, so a client never finds no instance.
   - An idle task runs every 1 s. If there are no connections and no live
     sessions for `VEAL_IDLE_MS` (default 30 000), exit.
3. `conn.rs`, one task per connection:
   - **Reader:** read into a buffer and `decode` frames. The first frame must be
     `Hello` with our `PROTOCOL`. Otherwise reply with an error and close.
   - **Writer:** a task owning the write half, fed by `Out`. `Out` is either
     `Frame(Vec<u8>)` or `Ready { id, client }`. On `Ready`, call
     `registry.take_from(id, client)` and send it as `snapshot` or `output`.
     `Ready` comes from the session's `Notify`, which sets a flag only once per
     drain, so there is never more than one pending.
   - **Per-connection map** `key -> (id, client)`, for `detach`, `resize` and
     `resync`.
   - **Dispatch:**
     - `Spawn` runs on `spawn_blocking`: resolve the proxy, using
       `resolve_proxy`, which moves from `lib.rs` into `core::proxy`. Then
       `registry.open(...)` with the `notify` above, and an `on_exit` that
       captured the key and broadcasts `Exit`. Broadcast `SessionAdded` if it
       spawned.
     - `Attach` is the same without spawning.
     - `Detach`, `Resize`, `Kill` (then `SessionRemoved`), `Forget`, `Resync`
       and `List` run inline.
     - `Theme` calls `set_theme_all` and stores the theme.
     - `Shutdown`: reply `Ok`, `kill_all`, wait up to 2 s for `pids()` to empty,
       then `process::exit(0)`.
     - `input` frames go to `registry.write(id, bytes)`. An error is logged, not
       replied to.
   - **Drop:** when the reader ends or a frame is bad, detach every client in the
     map (`registry.detach`, which also sends owed answers), remove the
     connection from `State`, and end the writer.
   - **Panics:** `catch_unwind` around dispatch, or run the connection task and
     check for a `JoinError` panic. Either way, log it and close that connection
     only.

Tests in the crate (`conn.rs`, with an in-memory duplex stream in place of the
pipe; tokio's `io::duplex`):
- `Hello` with the wrong protocol is refused.
- `Spawn` twice with one key gives one session.
- A fifth spawn is an error.
- A dropped connection detaches its clients and leaves the shell running.
- `Resync` gives a `snapshot` frame next.

Done when: the crate builds, its tests pass, and `veal-daemon.exe` started by hand
stays running with no window, then exits after 30 s idle.

### 5.6 Client (`core/src/client.rs`)

1. `Client::connect(name, sink: Arc<dyn Sink>) -> io::Result<Client>`:
   - `ClientOptions::new().open(name)`, then `check_server_owner`, then send
     `Hello`.
   - Await the reply, and return `Mismatch { daemon, build, sessions }` if the
     protocol differs.
   - `ERROR_PIPE_BUSY` means retrying after 20 ms, up to 1 s. `NotFound` is
     returned as is, for 5.7.
2. `Sink`:
   - `output(session, bytes)` and `snapshot(session, bytes)`
   - `event(DaemonMsg)`
   - `disconnected()`
3. Inside the client:
   - A writer task fed by an unbounded channel.
   - A reader task that decodes frames. A `Reply` goes to the oneshot in
     `pending: Mutex<HashMap<u64, oneshot::Sender<…>>>`, binary frames go to the
     sink, and events go to `sink.event`.
   - When the pipe closes, drain `pending` with a `Disconnected` error, then call
     `sink.disconnected()` once.
4. API:
   - `async request(msg) -> Result<ReplyBody, ClientError>`
   - `send(msg)` for `Detach`, `Resize` and `Resync`
   - `input(session, bytes)`
   - `async shutdown(timeout)`, which sends `Shutdown` and then waits for the
     pipe to close.
5. The sink is called on the reader task, and must not block. That is the
   bridge's job (5.9).

Tests: covered by 5.8. Add one unit test against a fake server on a duplex
stream: replies arriving out of order reach the right callers.

### 5.7 Starting the daemon (`core/src/client.rs`, `connect_or_launch`)

1. `connect_or_launch(exe: &Path, build: &str, sink) -> Result<Client, StartError>`:
   - Try `connect`. On `NotFound`, `launch(exe, build)` and then retry `connect`
     every 50 ms for 2 s.
   - On `Mismatch`, or a debug build whose `build` differs while the daemon has
     no sessions: `shutdown`, wait for the pipe to go, launch, connect.
   - A release mismatch is returned as `StartError::Mismatch { sessions }`.
2. `launch`:
   - `Command::new(exe).arg("--build").arg(build)`, with every stdio set to
     `Stdio::null()`.
   - `creation_flags(CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW | DETACHED_PROCESS)`.
     If that fails with raw OS error 5, log it and retry without
     `CREATE_BREAKAWAY_FROM_JOB`.
   - Do not keep the `Child`.
3. Finding the exe (the app's half, in `lib.rs`):
   - Release builds use `current_exe().with_file_name("veal-daemon.exe")`.
   - Debug builds (D6): the source is `target\debug\veal-daemon.exe`, next to the
     dev `veal.exe`. `build` = its file size and modified time as hex. Copy it
     to `veal-daemon-<build>.exe`, overwriting and ignoring an in-use error if
     that copy is already running, and launch the copy.
   - The daemon reports `build` in `Hello`.
   - Old copies with no running process are deleted best-effort on each launch.
4. If two apps race, both launch, one daemon exits on `AddrInUse`, and both
   connect to the winner. 5.8 tests this.

Done when: 5.8's start tests pass.

### 5.8 Integration tests (`src-tauri/daemon/tests/daemon.rs`)

Each test sets a unique `VEAL_PIPE` (from a UUID) and launches
`env!("CARGO_BIN_EXE_veal-daemon")` through `connect_or_launch`. It uses a test
`Sink` that collects bytes per session into a `Mutex<Vec<u8>>` and has a
`wait_for(session, needle, timeout)`. The shell is `cmd.exe`, as in `pty.rs`'s
tests.

| Test | Steps | Pass |
|---|---|---|
| reattach | spawn, `echo marker`, drop the client, reconnect, attach | the snapshot contains `marker`, and the shell's pid is unchanged in `list` |
| idempotent | spawn key A twice on two connections | one session in `list` |
| cap | spawn 5 keys | the fifth is an error |
| tombstone | spawn, write `ping -n 2 127.0.0.1 >nul & exit 3`, drop the client at once, wait 3 s, reconnect and attach | the reply says `exited`, code 3 |
| resync | flood `for /L`, with the test sink not taking, then `Resync` | the next frame is `snapshot`, starting `ESC c` |
| stalled | conn 1 attaches and never reads. Conn 2 attaches and runs a 50 000-line loop | conn 2 sees the end within 10 s, and conn 1's pipe never blocks the child |
| bad frame | send a length of `u32::MAX` | that connection closes. A second connection still works |
| shutdown | spawn 2, `shutdown` | both pids are gone and the pipe is gone within 3 s |
| idle | `VEAL_IDLE_MS=500`, connect and disconnect with nothing spawned | the daemon has exited within 2 s |
| race | two threads `connect_or_launch` at once | both work, and one daemon pid |
| owner | (not portable) | `#[ignore]`, with a note pointing at acceptance 14 |

Done when: all of these pass 10 times in a row (`cargo test -p veal-daemon --
--test-threads=1` in a loop), because flaky pipe tests would hide real races.

### 5.9 The bridge (`src-tauri/src/bridge.rs`, `lib.rs`)

1. `Bridge`:
   - `client: Mutex<Option<Arc<Client>>>`
   - `panes: Mutex<Panes>`, where `Panes` is
     `{ by_app: HashMap<u32, Pane>, by_wire: HashMap<u32, u32>, by_key: HashMap<String, u32> }`
   - `next_id: AtomicU32`, which never resets (D10)
   - `theme: Mutex<Option<(rgb, rgb)>>`
   - the `AppHandle`
2. `Pane { key, wire, outbox: Vec<u8>, notified: bool, resyncing: bool }`.
3. `Sink` for the bridge:
   - `output(wire, bytes)`: look up the pane. If it is `resyncing`, drop the
     bytes. Otherwise append, and if the outbox would pass `OUTBOX_CAP`, clear
     it, set `resyncing`, `client.send(Resync)` and push nothing.
     If `notified` was false, set it and emit `pty-data { id }`. Only a lock and
     a memcpy, so it never blocks.
   - `snapshot(wire, bytes)`: clear `resyncing`, replace the outbox, notify.
   - `event`: `Exit` is mapped by key to the app id and emits `pty-exit`.
     `Agent` emits `pty-agent`. Added and removed are ignored in step 5.
   - `disconnected`: clear the client and the panes, and emit `pty-daemon-lost`.
4. `ensure_client()`: the existing client, or `connect_or_launch` (5.7). Then
   send the stored theme. Called by every command, so after a daemon dies the next
   `pty_open` (Restart) brings one back.
5. Commands, with the same names and arguments so the payload test holds:
   - `pty_open`: `ensure_client`, then `request(Spawn{...})` with the debug
     overlay (5.3). Make a `Pane` with a new app id, and return
     `OpenedPane { id, client: 0, exited, code, agent }`. The client id now lives
     in the daemon; JS still sends it back and the bridge ignores it.
   - `pty_read(id, client)`: take the outbox, clear `notified`, and return it as
     a `Response` as today.
   - `pty_write`: `client.input(wire, bytes)`.
   - `pty_resize`: send `Resize`.
   - `pty_detach`: send `Detach` and remove the pane.
   - `pty_kill` and `pty_forget`: requests.
   - `pty_list`: a request.
   - New `pty_theme(fg, bg)`: store it and send `Theme`.
   - These become `async fn` and await the client. `pty_read` stays sync.
6. `run()`:
   - Remove `.manage(Arc<PtyRegistry>)` and `agents::start`, and manage
     `Bridge`.
   - `setup` calls `ensure_client` on a spawned task, so a slow start does not
     block the window. Commands await the same connection.
   - `on_page_load(Started)` calls `bridge.detach_all()`, which sends `Detach`
     for every pane and clears them. It happens before any command from the new
     page, and over the same pipe, so it is ordered before the new attaches.
   - `Destroyed` runs `block_on(client.shutdown(2 s))` (D4).
7. Check that `pty.ts` needs no change apart from the theme call and
   `pty-daemon-lost` (5.10). `pty.test.ts`'s payload test still parses `lib.rs`,
   so keep the command functions in `lib.rs` as thin wrappers over `bridge`.

Tests (Rust, in `bridge.rs`, with a fake client trait so no pipe is needed):
- An overflow sends one `Resync`, then drops `output` until a `snapshot`.
- `pty_read` clears `notified`.
- After `disconnected`, a stale wire id from the old daemon is ignored.
- Exit mapped by key reaches the right app id.

Done when: `pnpm tauri dev` opens panes through the daemon. Check that the
process tree shows `pwsh` under `veal-daemon-*.exe`, not under `veal.exe`, and
step 4's F5 checks still pass by hand.

### 5.10 UI (`src/pty.ts`, `src/components/TerminalPane.tsx`, `src/App.tsx`)

1. `pty.ts`:
   - `onDaemonLost(cb)`, with one app-wide listener for `pty-daemon-lost`. It
     also clears `exitHandlers`, `earlyExits`, `agentHandlers`, `latestAgents`
     and `dataHandlers`, since those ids are dead.
   - `setTheme(fg, bg)` calls `invoke("pty_theme")`.
2. `TerminalPane`:
   - A `lost` state. When the daemon is lost, stop the drain (`detach` is
     already a no-op on the Rust side) and show
     `<div className="pane-footer error">background service stopped <button>Restart</button></div>`,
     next to the existing footers at `TerminalPane.tsx:269`.
   - Restart bumps a `generation` counter in the open effect's dependencies.
     The effect then calls `openPty` with the same key and the saved cwd, which
     spawns fresh because the new daemon does not know the key.
3. Theme: an `App`-level effect on `useEffectiveMode()` calls
   `setTheme(hexToRgb(terminalTheme().foreground), hexToRgb(...background))`.
   Do it once, not per pane, since the daemon applies it to every session.

Tests (vitest, in the `pty.test.ts` style):
- `pty-daemon-lost` fires every `onDaemonLost` callback and empties the maps.
- Restart calls `pty_open` with the same key.
- `setTheme` sends the right arguments.
- `hexToRgb` parses `#rrggbb` and `#rgb`.

Done when: ending `veal-daemon-*.exe` in Task Manager shows the footer on every
pane, and Restart gives a working shell in the same cwd.

### 5.11 Dev wiring (`scripts/build-daemon.mjs`, `tauri.conf.json`)

Land this before 5.9.

1. `build-daemon.mjs`:
   - Run `cargo build -p veal-daemon --manifest-path src-tauri/Cargo.toml`,
     adding `--release` when it is passed `--release`.
   - Stream the output, and exit with cargo's code.
   - A header comment says step 7 adds the copy to
     `binaries/veal-daemon-x86_64-pc-windows-msvc.exe` for `externalBin`.
2. `tauri.conf.json`: set `beforeDevCommand` to
   `node scripts/build-daemon.mjs && pnpm dev`. `beforeBuildCommand` is left
   alone until step 7.
3. Add a `pnpm` script, `daemon`, for building it alone.

Done when: on a clean `target`, `pnpm tauri dev` builds the daemon and the app
starts it. Editing a daemon file and restarting dev picks up the new daemon
(D6's build check), as long as no panes are open.

### 5.12 Live checks

Build with `node scripts/build-daemon.mjs --release` and then
`pnpm tauri build`, so both exes sit in `target\release` (from step 7 on,
`pnpm bundle` does both, and its installer carries the daemon). Run `veal.exe`
from there. Use step 4's CDP scripts (`Page.reload`) where a check is scripted.

1. Step 4's live table, every row. The "close the app" row now means the daemon
   exits too (D4).
2. Acceptance 3 (F5 mid-agent, no junk in its input).
3. Acceptance 4: end `veal.exe` in Task Manager mid-run. The agent keeps going,
   and relaunching restores it. It is new in step 5, and it is the step's main
   result.
4. Acceptance 5: end the daemon. The footer and Restart work (5.10).
5. Acceptance 7: `exit` and `exit 3` while attached and while detached. Use F5 for
   "detached", since closing the window still sends `shutdown`.
6. Acceptance 10: plan-15's flood list, A2, A3 and A6, with four panes on one
   pipe. Pause the page in the debugger mid-flood: the child finishes, and the
   screen is right on resume.
7. Acceptance 11: `setx` a new `PATH` entry, open a new pane, and `where` finds
   the tool with no daemon restart.
8. Acceptance 13: plan-18's A1 and A10, and the Settings row matches a new pane's
   `env`.
9. Acceptance 15: four idle panes. Daemon CPU stays under 1% in Task Manager for 60
   s. Report private bytes with 5000 lines each.
10. Acceptance 16: sleep with an agent running, resume, type into it.
11. Known unknowns:
    - **Console flash.** Watch the screen during 10 spawns, and check that
      `Get-Process conhost` gains no window (`MainWindowHandle` stays 0).
    - **Breakaway.** Launch `veal.exe` from Windows Terminal, from VS Code's
      terminal and from Explorer. Close the launching terminal: the daemon must
      survive in all three. The log shows whether the fallback ran.
    - **Launchers.** Find the `veal` launchers that `pty.rs:228` refers to.
      Confirm they do not assume a shell dies with the window.
12. Write the results at the top of this file, as for steps 2–4, with test
    counts, the installer size (no daemon in it yet, D5) and anything found.

## Step 6 tasks

Section 8's close guard, tray and setting. Updates, `externalBin` and the
installer hooks stay in step 7. So does the version-mismatch prompt, which only
an update can cause.

**Decisions this breakdown makes against section 8** (change them here if wrong):

- **E1. A window is "attached" when a `desktop` connection is open.** `hello`
  already carries `role`; the daemon counts connections whose role is
  `desktop`. A pane being attached is not needed: a window with no panes still
  owns the sessions and shows them.
- **E2. The tray polls.** Its thread runs a Win32 message loop with a 1 s
  `SetTimer`, and each tick reads `State::tray_view()`. Shells exiting, agents
  changing and connections opening all change it, from three different threads;
  a 1 s poll of two mutexes is cheaper than wiring three notifiers, and well
  under A15's budget.
- **E3. Keep running is a flag on the bridge.** JS calls
  `pty_close_mode { keep }` just before `destroy()`. `Destroyed` sends `shutdown`
  unless the flag is set, so every other way the window goes (a crash, Alt+F4
  racing the guard) still ends idle shells, as today.
- **E4. The tray launches `veal.exe` from next to the daemon's own exe.** In
  dev, the daemon copy (D6) sits in `target\debug` beside `veal.exe`, so the same
  rule holds. The single-instance plugin covers a click while the app starts.
- **E5. The setting only covers agents.** Unsaved files still always ask. With
  `keep` or `stop` saved and no unsaved files, closing does not show the dialog.
  The dialog's "Remember" checkbox and the Settings row write the same key.
- **E6. Stop from the tray asks with a `MessageBoxW`** (Yes/No, default No)
  when agents are running, and not otherwise. It then runs `State::shutdown()`.

Order: 6.1, then 6.2. 6.3–6.5 do not depend on 6.1–6.2 or on each other, except
that 6.5 needs 6.3 and 6.4. Each task is one commit, and `cargo test --workspace`,
`pnpm test` and clippy pass after each.

### 6.1 What the tray shows (`daemon/src/lib.rs`, `core/src/pty.rs`)

1. `PtyRegistry::agents() -> Vec<String>`: the detected agent of every live
   session.
2. `State` records each connection's role with its sender, so
   `State::desktops()` counts `desktop` connections (E1).
3. `pub struct TrayView { tooltip: String, agents: usize }` and
   `State::tray_view() -> Option<TrayView>`: `None` while a desktop is attached
   or no session is live. Tooltip `Veal: 2 agents running`, `Veal: 1 agent
   running`, or `Veal: 3 shells running` when no agent is.
4. Tests: no view with a desktop attached; a view once it detaches with a shell
   still live; none with nothing live; the tooltip wording for 0, 1 and 2 agents
   (a pure `tooltip(sessions, agents)`).

Done when: the tests pass.

### 6.2 The tray (`daemon/src/tray.rs`, `daemon/Cargo.toml`)

1. Dependencies: `tray-icon` 0.24 (in the lock already), `png` 0.17 to decode
   the icon, and `windows-sys` with `Win32_UI_WindowsAndMessaging`.
2. `tray::start(state)`: a thread named `tray` running a message loop. A
   `SetTimer` of 1 s; each tick reads `tray_view()` (E2):
   - `Some` and no icon: build it from `icons/32x32.png` (`include_bytes!`),
     with the menu below. A failure is logged once and retried on the next
     tick, never fatal.
   - `Some` and an icon: update the tooltip if it changed.
   - `None`: drop the icon, which removes it.
3. Menu: **Open Veal**, **Stop everything and quit**. A left click on the
   icon is Open Veal.
   - Open: `Command::new(current_exe().with_file_name("veal.exe"))` (E4),
     detached, cwd the user's profile. Logged on failure.
   - Stop: if `agents > 0`, `MessageBoxW` "N agents are running. Stop them and
     quit?" Yes/No, default No (E6). Then `State::shutdown()` off the tray
     thread.
4. `main.rs` starts it after `run` has the pipe, so a second daemon that
   loses `first_pipe_instance` never shows an icon. `run` gains an
   `on_listening` callback for that.

Done when: it builds, clippy is clean, and the live check in 6.6 shows it.

### 6.3 Keep running on close (`src-tauri/src/lib.rs`, `bridge.rs`, `src/pty.ts`)

1. `Bridge::keep_on_close: AtomicBool`, and a command
   `pty_close_mode { keep: bool }` that sets it (E3).
2. `Destroyed`: if set, `detach_all()` and nothing else; the pipe closes with
   the process and the daemon detaches what is left. Otherwise `shutdown()` as
   today.
3. `pty.ts`: `setCloseMode(keep)`.
4. The payload test picks up the new command name.

Done when: tests pass.

### 6.4 The setting (`src/settings/store.ts`, `SettingsView.tsx`)

1. `closeWithAgents: "ask" | "keep" | "stop"`, key `close_with_agents`,
   default `ask`, anything else loads as `ask`.
2. A Settings row, "When closing with agents running", a segmented control:
   Ask, Keep running, Stop them.
3. Tests in `store.test.ts`: default, parse, write-through.

Done when: tests pass.

### 6.5 The close guard (`src/close.ts`, `CloseGuard.tsx`)

1. `src/close.ts`, pure:
   `closeDecision({ dirty, agents, setting }) -> "stop" | "keep" | "ask"`.
   Unsaved files ask. No agents: `stop`. Otherwise the setting, with `ask`
   asking (E5).
2. `CloseGuard`: `stop` and `keep` call `setCloseMode`, `flushAll`, `destroy`.
   The dialog, when agents are running, reads "These keep running in the
   background, and Veal stays in the tray:" with three buttons: Cancel,
   **Stop and quit**, **Keep running** (focused, primary). With unsaved files the
   two quits are "Discard and stop", "Discard and keep running". With unsaved
   files and no agents it is as today. A "Don't ask again" checkbox saves the
   chosen one into `closeWithAgents`.
3. Tests in `close.test.ts`: one per row of the decision.

Done when: tests pass.

### 6.6 Live checks

On the release build, as for 5.12:

1. Acceptance 1, with a `claude` session doing a long task: close, Keep
   running. The tray shows it with the right tooltip. Reopen from the tray
   after 5 minutes: same pane and screen, badge and dot right at once, work
   finished.
2. Acceptance 2: prompt, Shift+Enter, paste text, paste an image, Ctrl+C in the
   reattached agent.
3. Acceptance 6: close with idle shells only. No dialog, no tray, daemon gone.
4. Acceptance 8: spawn a pane and close within a second, Keep running: the
   shell starts and is usable on reopen.
5. Acceptance 9: close mid-spawn 20 times, no session hangs.
6. Tray: Stop asks with an agent running and not without; the icon goes when
   a window attaches; the setting's `keep` and `stop` skip the dialog.
7. Write the results at the top of this file.

## Step 7 tasks

Section 8's updates and packaging: the installer carries the daemon, an update
stops it first, the installer and uninstaller stop it as a backstop, and a daemon
of another protocol gets a prompt instead of a pane error. Publishing and the
version bump stay in step 8.

**Checked against Tauri at the start of this step** (the known unknown):

- CLI 2.11.4, `tauri-build` 2.6.3, `tauri-utils` 2.9.3.
  `bundle.windows.nsis.installerHooks` exists. It takes a `.nsh` file with
  `NSIS_HOOK_PREINSTALL`, `POSTINSTALL`, `PREUNINSTALL` and `POSTUNINSTALL`
  macros.
- In the NSIS template, `NSIS_HOOK_PREINSTALL` runs **before**
  `CheckIfAppIsRunning "veal.exe"`, and that check looks only at the main exe,
  never at a sidecar. The template sets `$PassiveMode` (`/P`, which the updater
  passes) and `$UpdateMode` (`/UPDATE`). `nsis_tauri_utils::FindProcess` is
  available to hooks.
- `externalBin: ["binaries/veal-daemon"]` takes
  `binaries/veal-daemon-x86_64-pc-windows-msvc.exe` and installs it as
  `veal-daemon.exe` next to `veal.exe`, which is where `client::launch` looks.
- `tauri-build` copies every `externalBin` into `target\<profile>\` on **every**
  cargo build of `veal`: dev, `cargo test` and release alike. It removes the
  old file with `unwrap()` and fails if the source is missing.

**Decisions this breakdown makes against section 8** (change them here if wrong):

- **F1. `externalBin` and the hooks live in a bundle-only config,**
  `src-tauri/tauri.bundle.conf.json`, merged with `tauri build --config`. In the
  main config, the copy above would break a fresh clone with no `binaries/` file,
  and could overwrite `target\debug\veal-daemon.exe`, which `cargo test` just
  built, with a stale copy. It would also panic the build whenever a daemon is
  running from that file. The CLI hands the merged config to `tauri-build`, so
  only bundling sees it. `pnpm bundle` and `release.mjs` always pass it.
- **F2. The pre-install hook asks about `veal.exe` before it touches the
  daemon.** It runs the template's own `CheckIfAppIsRunning` first. Otherwise a
  user who cancels at "Veal is running" would already have lost every shell. The
  template's later check then finds nothing.
- **F3. The daemon is stopped gracefully, then by force.**
  `veal-daemon.exe --stop` connects to the user's pipe and sends `shutdown`,
  which is allowed even on a protocol mismatch, then waits up to 3 s for the pipe
  to go. The hook runs the **installed** (old) exe, which matches the running
  daemon. If the daemon is still there after that, `taskkill /F /IM
  veal-daemon.exe` runs.
- **F4. The installer only asks when a person is at it.** When `$PassiveMode`
  or `$UpdateMode` is set, the app has already warned and stopped the daemon,
  so the hook stops anything left without asking. A manual install or uninstall
  with the daemon running asks first ("Installing ends every shell Veal is
  running in the background", OK/Cancel; Cancel aborts).
- **F5. The update is tested under a separate identity.** The test builds merge
  `tauri.test.conf.json`: `productName` "Veal Test", `identifier`
  `com.veal.test`, versions 0.1.90 and 0.1.91, an updater endpoint on
  `http://127.0.0.1:8765/latest.json` with `dangerousInsecureTransportProtocol`,
  and signing with `~/.tauri/veal-updater.key`. The installed Veal and its
  `veal.db` are not touched. The pipe name is per user, not per identity, so the
  real Veal must not run during the test. Nothing is published.
- **F6. Mismatch Restart works over the old protocol.** `list` and `shutdown` are
  the two messages a mismatched client may send (5.5). So Restart sends
  `shutdown` to the old daemon, waits for its pipe to go, and connects again,
  which launches the new one.

Order: 7.1, then 7.2 (its hook needs the bundled exe). 7.3 comes before 7.2's
hook can be tried. 7.4 and 7.5 do not depend on the others. Each task is one
commit, and `cargo test --workspace`, `pnpm test` and clippy pass after each.

### 7.1 Bundle the daemon (`scripts/build-daemon.mjs`, `tauri.bundle.conf.json`, `release.mjs`)

1. `build-daemon.mjs --sidecar`: after the release build, copy
   `target\release\veal-daemon.exe` to
   `src-tauri\binaries\veal-daemon-x86_64-pc-windows-msvc.exe`. If the copy
   fails because the file is locked, say which process holds it. Add
   `src-tauri/binaries/` to `.gitignore`.
2. `src-tauri/tauri.bundle.conf.json` (F1):
   - `build.beforeBuildCommand`: `node scripts/build-daemon.mjs --release
     --sidecar && pnpm build`
   - `bundle.externalBin`: `["binaries/veal-daemon"]`
   - `bundle.windows.nsis.installerHooks`: `windows/hooks.nsh` (an empty file
     until 7.2)
3. `package.json`: `"bundle": "tauri build --config
   src-tauri/tauri.bundle.conf.json"`. `release.mjs` runs `pnpm bundle`
   instead of `pnpm tauri build`.
4. `release.mjs` checks, before it signs off, that the generated
   `target\release\nsis\x64\installer.nsi` names `veal-daemon.exe`. A plain
   `pnpm tauri build` makes an installer with no daemon, and that check stops it
   from being released.
5. Update the comment at the top of `build-daemon.mjs`, and step 5.12's build
   line, to `pnpm bundle`.

Done when: `pnpm bundle` makes an installer whose `installer.nsi` names the
daemon; `release.mjs --dry-run` passes; `cargo test --workspace` passes on a
checkout with no `binaries/` folder; `pnpm tauri dev` still starts a daemon.

### 7.2 Installer hooks (`src-tauri/windows/hooks.nsh`)

1. A `StopDaemon` macro:
   - `nsis_tauri_utils::FindProcess "veal-daemon.exe"`. If it is not running,
     stop here.
   - Unless `$PassiveMode` or `$UpdateMode` is set (F4), `MessageBox
     MB_OKCANCEL|MB_ICONEXCLAMATION` "Installing ends every shell Veal is running
     in the background." Cancel: `Abort`.
   - `nsExec::Exec '"$INSTDIR\veal-daemon.exe" --stop'` if that file exists
     (F3).
   - `FindProcess` again. If it is still running,
     `nsExec::Exec 'taskkill /F /IM veal-daemon.exe'`, then wait up to 2 s for it
     to go.
2. `NSIS_HOOK_PREINSTALL`: `!insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe"
   "${PRODUCTNAME}"` (F2), then `StopDaemon`.
3. `NSIS_HOOK_PREUNINSTALL`: the same, with the message saying "Uninstalling".
4. Check that the generated `installer.nsi` places the hook ahead of the file
   copy, and that the macros compile (makensis runs as part of `pnpm bundle`).

Done when: the bundle builds. The behaviour is checked in 7.6.

### 7.3 `veal-daemon.exe --stop` (`daemon/src/main.rs`, `core/src/client.rs`)

1. `client::stop(name) -> Stopped`, where `Stopped` is one of `NotRunning`,
   `Stopped` or `StillRunning`. It connects with role `installer` and sends
   `Shutdown`, which is valid on any protocol. Then `wait_gone(name)`, which
   gets a `bool` result (today it returns nothing), up to 3 s.
2. `main.rs`: `--stop` (with `--pipe` optional, as now). It runs `client::stop`
   on a current-thread runtime and exits 0, 1 or 2 to match. It never starts a
   daemon or a tray.
3. Integration tests: `--stop` on a test pipe ends a running daemon and its
   shells (exit 0); on a pipe with no daemon it exits 1.

Done when: the tests pass.

### 7.4 Stop the daemon before an update (`UpdateCard.tsx`, `lib.rs`, `bridge.rs`, `pty.ts`)

1. `Bridge::shutdown_and_wait()`: `shutdown`, then wait for the pipe to go (up to
   3 s). It returns whether it went. Command `pty_shutdown { }`, and
   `pty.ts: shutdownDaemon()`.
2. `UpdateCard.install`: download first, then `flushAll`, then
   `setCloseMode(false)`, then `await shutdownDaemon()`, then `update.install()`.
   A shutdown that times out is logged and the install goes ahead: the hook is
   the backstop (F3, F4).
3. `atStake`'s warning stays, since agents are lost on update. Reword it to say
   so: "Updating stops Veal's background service: 2 running agents will end."
   Unsaved edits are listed as before.
4. If `install()` throws after the shutdown, the panes show "background service
   stopped" with Restart (5.10) and the card shows the error. Nothing else is
   needed.
5. Tests: `pty.test.ts` checks the command name and keys against `lib.rs`.
   A test of the order (download, flush, close mode, shutdown, install) mocks
   the updater and `pty`.

Done when: tests pass.

### 7.5 The version-mismatch prompt (`bridge.rs`, `lib.rs`, `App.tsx`, new `DaemonMismatch.tsx`)

1. `Bridge::client()` keeps `StartError::Mismatch { sessions }` as a kind, not
   only a string. It emits `pty-daemon-mismatch { sessions }` once per
   connect attempt, and pane opens fail with the same text as now.
2. Command `pty_replace_daemon`: connect without the version check, send
   `Shutdown`, wait for the pipe to go, then connect normally, which launches
   this build's daemon (F6).
3. `DaemonMismatch.tsx`: a modal, "2 sessions are running on an older
   background service. Restart ends them and starts this version's." Buttons:
   **Quit** (closes the window, sending nothing) and **Restart** (focused).
   After Restart, the panes remount, as after the 5.10 footer's Restart.
4. A test-only `--protocol N` flag in the daemon makes a mismatched daemon for
   the tests. It is compiled only with `debug_assertions`.
5. Tests: an integration test showing that `connect_or_launch` against a
   `--protocol 99` daemon with a session gives `Mismatch`, and that the replace
   path then ends it and connects to a new one. A TS test of the modal's two
   buttons.

Done when: tests pass.

### 7.6 Live checks

Under the test identity (F5), with the real Veal closed. Build 0.1.90 and 0.1.91
with `pnpm bundle --config src-tauri/tauri.test.conf.json`. Serve 0.1.91's
installer and `latest.json` from `127.0.0.1:8765`.

1. Install 0.1.90. Its install folder has `veal.exe` and `veal-daemon.exe`.
   A pane works, and the daemon runs from the install folder.
2. Acceptance 12, update: with an agent running in 0.1.90, the update card
   warns, and Update anyway stops the daemon. 0.1.91 installs with no prompt,
   relaunches, and starts its own daemon (log: new build, new PID). No old
   process is left.
3. Manual install over a running app: run 0.1.91's installer with 0.1.90 open
   and an agent running. "Veal is running" comes first; cancelling it leaves
   the agent running. Running it again and accepting both prompts installs.
4. Manual install with only the daemon left (Keep running, window closed): the
   shells prompt appears, then the install goes through, with no daemon left.
5. Acceptance 12, uninstall: with the daemon running, uninstall asks, then
   removes the folder completely. No `veal-daemon.exe` process and no locked
   file remain.
6. Mismatch, on a debug build: start a `--protocol 99` daemon with a shell on
   the dev pipe, then launch the app. The prompt shows the session count;
   Restart gives working panes on a new daemon.
7. `pnpm release --dry-run` passes and the installer it builds carries the
   daemon.
8. Clean up: uninstall Veal Test, and delete `%APPDATA%\com.veal.test` and
   `%LOCALAPPDATA%\com.veal.test`.
9. Write the results at the top of this file. Left for a person: SmartScreen
   and antivirus on a clean machine (known unknown).

## Step 8 tasks

The acceptance list below, run on one release candidate, then the version bump and
the release. Most rows were checked on earlier builds in steps 5–7. Since steps 6
and 7 changed closing and packaging, every row that can be scripted is run again
on the candidate. The rows that need a person get exact steps. Nothing is published
until the user says so.

Where things stand: `claude/daemon-step-4` is 37 commits ahead of `origin/main`.
The published release is v0.1.2, from 2026-10-16, with no daemon. The version is
0.1.2 in five places: `package.json`, `tauri.conf.json`, and the `veal`,
`veal-core` and `veal-daemon` crates.

**Decisions this breakdown makes** (change them here if wrong):

- **G1. The live-check scripts go into the repo,** in `scripts/acceptance/`:
  `cdp.mjs`, `closewin.ps1`, `uia.ps1`, `old-session.mjs` and a stand-in agent
  maker. They have been rebuilt in a scratch folder for each of steps 5–7. They
  are tools for a person or a session to run, not part of `pnpm test`.
- **G2. One candidate, every scriptable row.** Build once with `pnpm bundle` and
  the test identity, and run rows 3–11, 13 and 15 against it. A row that fails
  gets fixed, and the candidate is rebuilt and every row run again: a fix can
  break a row that passed.
- **G3. The version goes to 0.1.3 in all five places,** with a test that keeps
  them equal. It also fixes 7.6's note: the daemon's build string comes from its
  own crate version, so it now moves with the app's.
- **G4. The upgrade from v0.1.2 is tested from v0.1.2's own source.** The tag's
  code is built under the test identity at 0.1.89 and updated to the candidate at
  0.1.92. That covers the path every current user takes (no daemon, to a daemon,
  through the old app's updater and the new installer's hooks) without touching
  the installed Veal. Re-running it with the published v0.1.2 binary would replace
  the real install, so that is left to the user.
- **G5. Release order:** PR to `main`, the user merges it, then `pnpm release`
  runs from an up-to-date `main` checkout. Publishing and merging both wait for
  the user.

Order: 8.1 and 8.2 first, in either order. 8.3 needs both. 8.4 needs the
candidate from 8.3. 8.5 can be done while 8.3 and 8.4 run. 8.6 comes last.

### 8.1 Acceptance scripts (`scripts/acceptance/`)

1. Move in the scratch tools, cleaned up and with a usage line each:
   - `cdp.mjs <port> eval|type|enter|shot`
   - `closewin.ps1` (WM_CLOSE to the window)
   - `uia.ps1 list|click <pid> <name>` (NSIS dialogs through `BM_CLICK`)
   - `old-session.mjs <pipe>` (a protocol-99 session)
   - `fake-agent.ps1`, which copies `ping.exe` to `claude.exe` in a folder the
     script is given
2. Give `closewin.ps1` an optional exe path, so it closes the right `veal.exe`
   when a test install and a dev build are both running.
3. `scripts/acceptance/README.md`: the CDP launch variable
   (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`), the test identity, the feed server
   line, and that the scripts run against the user's own data unless the test
   identity is used.

Done when: each script runs once from its new place.

### 8.2 Version 0.1.3 (G3)

1. Set 0.1.3 in the five places, and refresh `Cargo.lock`.
2. `src/version.test.ts`: reads the five files raw, as the payload test reads
   `lib.rs`, and checks that they agree.
3. Do not tag or publish (G5).

Done when: tests pass and `cargo build` picks up the lock change.

### 8.3 Release candidate and scripted acceptance (G2)

Build the candidate with `pnpm bundle --config src-tauri/tauri.test.conf.json
--config '{"version":"0.1.92"}'`, signed. Install it with `/S`, with the real Veal
closed. Rows, numbered as in the acceptance list:

1. **3.** F5 twice with the agent running: same PID, badge right, no junk typed
   into it (its pane shows nothing new).
2. **4.** End `veal.exe` in Task Manager (`Stop-Process`): agent and daemon live;
   relaunch reattaches.
3. **5.** End `veal-daemon.exe`: every pane shows "background service stopped";
   Restart gives shells in the same cwd.
4. **6.** Close with idle shells: no dialog, no tray, daemon gone.
5. **7.** `exit` attached closes the pane. `exit` while detached (F5) is dropped
   on relaunch. `exit 3` while detached shows `process exited (code 3)`.
6. **8.** New terminal then close at once, with Keep running: the new pane works
   on reopen.
7. **9.** That, 20 times: no hang, every pane answers.
8. **10.** plan-15's flood in four panes for 60 s: none stops, all answer after
   Ctrl+C. Pause the page in the debugger mid-flood; the screen is right on
   resume.
9. **11.** `setx` a new `PATH` entry with a test tool in it (then remove it). A
   new pane finds the tool with the same daemon PID.
10. **13.** plan-18's A1 and A10, and the Settings row equals a new pane's
    proxy variables.
11. **15.** Four idle panes: daemon CPU under 1% over 60 s. Report private bytes
    with 5000 lines each.
12. Closing and tray, from step 6: Keep running leaves the agent running and the
    tray icon up, shown in the log; reopening hides it; the saved `keep` and
    `stop` settings skip the dialog.

Each row gets a result line. A failure is fixed in its own commit, then the
candidate is rebuilt and every row run again.

Done when: every row passes on one candidate.

### 8.4 Upgrade from v0.1.2 (G4)

1. A temporary worktree at tag `v0.1.2`. Build it signed with the test identity
   at version 0.1.89 (`tauri build` with the same `--config`; it has no daemon to
   bundle). Remove the worktree afterwards.
2. Serve the 8.3 candidate (0.1.92) as the feed. Install 0.1.89 with `/S`, pin a
   project, and open two panes. Its `veal.db` gets a pre-step-4 layout.
3. Update from inside 0.1.89, with a long-running command in a pane. Its card
   warns as v0.1.2 did. The installer runs passively, and the hooks find no
   daemon and ask nothing.
4. 0.1.92 starts. The two panes come back from the old layout with new session
   keys (`parseNode`), in their saved folders, and a daemon starts from the
   install folder.
5. Close with Keep running, then reopen: it still works on data that started on
   0.1.89.
6. Uninstall, and clean up as in 7.6.

Done when: every step passes.

### 8.5 Checks for a person (`plan-19-checks.md`)

A one-page checklist with exact steps and a box for the result, so it can be done
without reading this plan:

1. **Acceptance 1 and 2:** a real `claude` on a long task. Close with Keep
   running, check the tray tooltip, reopen from the tray after 5 minutes: same
   screen, badge right, the work went on. Then a prompt, Shift+Enter, a text
   paste, an image paste, and Ctrl+C.
2. **The tray:** left click opens Veal; Open Veal; Stop everything asks with an
   agent running and not without; the icon and tooltip look right.
3. **Acceptance 14:** from a second local Windows account, opening
   `\\.\pipe\veal-<first user's SID>` fails. It includes a one-line PowerShell
   for the attempt.
4. **Acceptance 16:** sleep with an agent running, wake, and type into it.
5. **Carried over:** launching from VS Code's terminal and closing it; watching
   for a console flash in 10 spawns; three `claude` panes for 20 minutes
   (plan-15 A4); mouse-wheel scrolling in a `claude` pane (A5).
6. **Clean machine:** install the candidate on a PC or VM that has never had
   Veal, with Defender on. Note any SmartScreen prompt and whether
   `veal-daemon.exe` is flagged or slowed.
7. **Optional:** update the installed v0.1.2 to the candidate for real (G4's
   caveat).

Done when: the file exists. Its results come back from the user.

### 8.6 Release (G5)

1. Write the results at the top of this file, and fold the user's checklist
   results in.
2. Open a PR from `claude/daemon-step-4` to `main`, with a summary of steps 2–8
   and the checklist results. Do not merge it.
3. After the user merges it: from an up-to-date `main`, run
   `pnpm release --notes "…"` with notes written from the step results. Only on
   the user's go-ahead, since it publishes v0.1.3 to everyone on the updater.
4. After publishing: the installed Veal updates itself from v0.1.2. Check that
   its first launch starts a daemon and keeps the layout.

Done when: v0.1.3 is published and the installed Veal is on it.

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
- ~~Tauri's NSIS details.~~ Checked 2026-11-09 against 2.11.4: `installerHooks`
  exists, and the running-app check ignores sidecars. See "Step 7 tasks".

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
- 2026-11-09: each step 5 task given a how-to (files, steps, tests, done when), and
  decisions D7–D10: `spawn` attaches, a snapshot frame kind, a writer thread per
  session, and app-side ids.
- 2026-11-09: step 5 built and checked live on the release build. Results at the
  top; three checks left for a person.
- 2026-11-09: step 6 broken into tasks (section "Step 6 tasks"), with decisions
  E1–E6: a desktop connection means attached, the tray polls, Keep running is a
  flag on the bridge, and the setting only covers agents.
- 2026-11-09: step 6 built and checked live on the release build. Results at the
  top; real-agent and tray-click checks left for a person.
- 2026-11-09: step 7 broken into tasks (section "Step 7 tasks"). Tauri's NSIS
  hooks and sidecar naming checked against 2.11.4; `externalBin` moved to a
  bundle-only config (F1), since `tauri-build` copies it on every cargo build.
- 2026-11-10: step 7 built and checked live under the test identity. Results at
  the top; the clean-machine check left for a person.
- 2026-11-10: step 8 broken into tasks (section "Step 8 tasks"): the scripts into
  the repo, version 0.1.3 everywhere, one candidate for every scriptable row, the
  upgrade from v0.1.2 built from its tag, a checklist for a person, then a PR and
  the release on the user's word.
- 2026-11-10: step 8 tasks 8.1–8.5 done. Every scriptable row passes on a rebuilt
  0.1.92 candidate after one fix (the stale agent badge after Restart), the upgrade
  from v0.1.2's code works, and `plan-19-checks.md` holds the checks for a person,
  now including row 11. Results at the top.
