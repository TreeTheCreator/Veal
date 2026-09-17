# Veal — plan for step 16: terminal output that cannot stall

Status: BUILT 2026-09-18, not yet committed.

Results: `pnpm test` 155, `cargo test` 51, `pnpm build` green. Dev window
over CDP: one pane flooding at ~170 KB/s of rendered output for 20 s and
four panes at ~195 KB/s each for 60 s, none stalled, every pane answered
`echo` after Ctrl+C; UI-thread lag during the four-pane flood p50 0.8 ms,
max 3 ms; veal.exe private bytes 9 to 10 MB, the WebView2 processes +56 MB
(includes 4 x 5000 lines of new scrollback). Closing a pane mid-flood
removed it and its shell within 2.5 s. Real wheel input over a flooded
pane scrolls 3 lines per notch. Acceptance 4 (20 min of real agent use)
is the user's.

Reported 2026-09-18 while running three Claude Code panes: every terminal
froze at once. Panes still accepted keystrokes (the write path is a plain
invoke) but nothing came back, the last Ink frame stayed half-drawn, and
the pane could not be scrolled into anything readable. The "cannot
scroll" symptom is downstream of the freeze: what was on screen was a
partial redraw, not scrollback.

## Cause

Output reaches xterm through one `tauri::ipc::Channel` per pane, one
message per 16 KiB `read()` (`pty.rs`, `lib.rs::pty_spawn`). Two things
about that path, read from tauri 2.11.5 `src/ipc/channel.rs`:

1. **Every chunk is a `webview.eval`.** Below 1024 bytes the bytes are
   inlined as a JSON number array; at or above, the eval kicks off a
   fetch (`__TAURI_CHANNEL_FETCH__`) whose response is handed to the
   channel callback. Three agents redrawing their UI is hundreds of evals
   a second on the WebView2 UI thread, with no coalescing and no
   backpressure (tauri#12724 is the same shape: sustained emits grow
   without bound).
2. **Message order is enforced in JS with no recovery.** `Channel`
   (`@tauri-apps/api/core.js:74`) processes message *N* only after
   *N−1*; the fetch path resolves asynchronously and ends in
   `.catch(console.error)`. One fetch that fails or never resolves —
   under exactly the load in (1) — and every later message for that pane
   queues in `pendingMessages` forever. The PTY keeps reading, the
   channel never delivers again: a permanent, silent freeze. Three panes
   under the same load stall within the same second.

(2) is the freeze; (1) is why it happens under agent load and not at a
prompt. The fix removes the Channel rather than tuning it: no per-chunk
eval, no ordering gate, and output that xterm has not consumed applies
backpressure to the shell, as a real terminal does.

## Design: pull, not push

Rust buffers; JS drains. Per pane, an `Outbox` (`pty.rs`):

```rust
struct Outbox {
    buf: Vec<u8>,          // bytes read but not yet fetched by JS
    notified: bool,        // a `pty-data` event is outstanding
    closed: bool,          // releases a blocked reader on exit/kill
}
// behind Mutex + Condvar in PtyHandle
```

- **Reader thread**: `read()` → lock → if `buf.len() >= CAP`, wait on the
  condvar until it drops below (or `closed`) → append → if `!notified`,
  set it and emit `pty-data {id}`. So a pane that is being drained gets
  *no* events at all; one event marks each idle→busy edge.
- **`pty_read(id) -> tauri::ipc::Response`**: lock, `take()` the buffer,
  `notified = false`, notify the condvar, return the bytes. Tauri's
  `Response::new(Vec<u8>)` reaches JS as an `ArrayBuffer` over the ipc
  protocol — the binary path, no JSON, one round trip per drain however
  many `read()`s it covers.
- **JS drain loop** (`pty.ts`): on `pty-data` for an id, start a loop if
  none is running, else flag `again`:
  ```
  do { again = false
       bytes = await invoke("pty_read")
       if (bytes.length) await write(bytes)   // term.write(bytes, cb)
  } while (bytes.length || again)
  ```
  `term.write`'s callback fires when xterm has parsed the bytes, so the
  next fetch waits for xterm, which waits for the renderer: the whole
  chain has backpressure, ending at the child's `write(2)`.
- **Exit / kill**: set `closed`, notify, drop the buffer. The waiter
  thread's `drop(handle)` (ClosePseudoConsole, which blocks until output
  is drained) is only reached once the reader has been released, so a
  pane that JS stopped draining still closes.
- **Missed-wakeup proof**: `notified` is set and the event emitted under
  the same lock that `pty_read` clears it in. Any append after a take
  finds `notified == false` and emits; any append before it finds a loop
  that is about to `read` again.

## Numbers

- `CAP` = 1 MiB per pane. An agent redraw is tens of KB; 1 MiB is
  seconds of the heaviest flood we can produce, and 4 panes × 1 MiB is
  nothing. Below it, no backpressure ever engages in practice; at it the
  shell blocks instead of memory growing.
- `READ_BUF` stays 16 KiB.
- No debounce on the drain: the loop's own round trip (one invoke + one
  `term.write`) is the coalescing interval — ~1–4 ms idle, longer under
  load, exactly when batching helps.
- Event name `pty-data`, payload `{ id }` (PTY id, mapped to the pane by
  the session as `pty-agent` is).

## Changes

1. `pty.rs`: `Outbox` + `take(id) -> Vec<u8>`; the reader appends instead
   of calling `on_data`; `spawn` takes `on_data_ready: FnMut(u32)`
   instead of `on_data`. `kill`/waiter mark `closed`. The debug
   `chunks/s` line becomes `bytes/s drained` from `take`.
2. `lib.rs`: `pty_spawn` drops the `out` channel, emits `pty-data` from
   `on_data_ready`; new `pty_read` command returning
   `tauri::ipc::Response`.
3. `pty.ts`: `spawnPty(opts, onData)` keeps its signature; `onData`
   becomes `(bytes, done) => void` so `TerminalPane` can call
   `term.write(bytes, done)`. One app-wide `pty-data` listener feeding a
   per-id drain loop; the loop exits when the session is disposed.
   Because `pty_spawn` resolves *after* the shell may have emitted, the
   session starts one drain unconditionally on creation (same reasoning
   as plan-13's early-exit map).
4. `TerminalPane.tsx`: `spawnPty({...}, (bytes, done) => term.write(bytes, done))`.
5. `pty.test.ts`: `pty-data` before the spawn reply is not lost; a
   second event during a running loop causes one more `pty_read`, not a
   second loop; the loop stops when `pty_read` returns empty and
   restarts on the next event; dispose ends it.
6. `pty.rs` tests: `Outbox` unit tests (append sets `notified` once,
   `take` clears it, cap blocks a second appender until `take`, `closed`
   releases it); the existing echo test rewritten against `take`.

## Acceptance

1. `pnpm test`, `cargo test`, `pnpm build` green.
2. Flood, one pane: PowerShell `1..300000 | % { "line $_ ................" }`
   — the pane keeps rendering to the end, Ctrl+C stops it promptly
   (backpressure means the shell is never minutes ahead of the screen).
3. Flood, four panes at once, 60 s: no pane stops; the top bar and
   sidebar stay responsive; `veal.exe` private bytes stable (Task
   Manager, before/after within 50 MB).
4. Three Claude Code panes, 20 minutes of real use: no freeze.
5. Wheel over a pane scrolls xterm's scrollback; the Ink frame is whole.
6. `exit` in a flooding pane still closes it (the reader is released).
7. Commit: `Replace the per-chunk output Channel with a drained Outbox: backpressure, no ordering stall`.

## Decisions

1. **Pull model over coalescing on the Channel.** Batching to 64 KiB /
   16 ms (what other Tauri terminals do) cuts the evals but leaves the
   ordering gate: one lost fetch is still a dead pane. Pull has no
   sequence numbers to lose.
2. **Event on the idle→busy edge, not per chunk.** Per-chunk events are
   the eval storm again (and tauri#12724). A polling timer (16 ms) would
   also work but costs 60 invokes/s per idle pane.
3. **Block the reader at CAP** rather than drop or grow. Dropping
   corrupts escape sequences; growing is the leak we are removing. A
   blocked reader is what a terminal that is not being read does.
4. **`tauri::ipc::Response` for `pty_read`** rather than base64 in JSON:
   the ipc protocol already carries invoke responses as bytes; base64 is
   ×1.33 plus a decode on the UI thread.
5. **Keep `pty_write` as is.** Input is small and already ordered by
   invoke sequencing; the plan-12 writer lock stands.
