//! PTY plumbing: one real shell process per pane, driven by portable-pty.
//!
//! Design: no async runtime. portable-pty is blocking, so each pane owns two
//! `std::thread`s — a reader that pumps raw bytes to the frontend and a waiter
//! that blocks on `child.wait()`. The waiter is what tears the pane down: on
//! Windows the ConPTY reader only sees EOF once the pseudoconsole is closed,
//! which happens when we drop the master, so "child exited" has to be observed
//! independently of the byte stream.
//!
//! Output is pulled, not pushed (plan-15): the reader appends to a per-pane
//! `Outbox`, signals the frontend once per idle-to-busy edge, and the
//! frontend drains with `take` at its own pace. Above `OUTBOX_CAP` the reader
//! blocks, which blocks the child: backpressure, as a terminal that is not
//! being read applies it.
//!
//! The registry is Tauri-agnostic (callbacks instead of channels/events) so it
//! can be exercised from `cargo test` without a webview.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// Bytes per `read()` on the PTY master.
const READ_BUF: usize = 16 * 1024;

/// Unfetched bytes a pane may hold before its reader stops reading. An agent
/// redraw is tens of KB; this is seconds of the heaviest flood.
pub const OUTBOX_CAP: usize = 1024 * 1024;

/// Bytes read from the PTY that the frontend has not fetched yet.
#[derive(Default)]
struct Outbox {
    buf: Vec<u8>,
    /// A data-ready signal is outstanding: the frontend will drain, so no
    /// further signal is needed until it has.
    notified: bool,
    /// The reader hit EOF; nothing more will ever be appended.
    eof: bool,
}

/// One pane's outbox plus the flags that release a reader waiting at the cap.
#[derive(Default)]
struct Shared {
    out: Mutex<Outbox>,
    drained: Condvar,
    /// The frontend gave the pane up (`kill`): discard, never block.
    closed: AtomicBool,
    /// The child exited: the waiter must not be held up by a full outbox.
    exited: AtomicBool,
}

impl Shared {
    /// Append `bytes` unless the pane is closed; block while the outbox is
    /// full and the child is alive. Returns whether the frontend needs a
    /// data-ready signal.
    fn push(&self, bytes: &[u8]) -> bool {
        if self.closed.load(Ordering::Relaxed) {
            return false;
        }
        let mut o = self.out.lock().unwrap_or_else(|p| p.into_inner());
        while o.buf.len() >= OUTBOX_CAP
            && !self.exited.load(Ordering::Relaxed)
            && !self.closed.load(Ordering::Relaxed)
        {
            o = self.drained.wait(o).unwrap_or_else(|p| p.into_inner());
        }
        // Woken by close or exit with the outbox still full: drop the bytes.
        if self.closed.load(Ordering::Relaxed) || o.buf.len() >= OUTBOX_CAP {
            return false;
        }
        o.buf.extend_from_slice(bytes);
        let signal = !o.notified;
        o.notified = true;
        signal
    }

    /// Hand the frontend everything so far. Clears `notified` under the same
    /// lock `push` sets it, so an append after this take always signals.
    /// Returns `(bytes, finished)`; finished means EOF was reached.
    fn take(&self) -> (Vec<u8>, bool) {
        let mut o = self.out.lock().unwrap_or_else(|p| p.into_inner());
        let bytes = std::mem::take(&mut o.buf);
        o.notified = false;
        let finished = o.eof;
        drop(o);
        self.drained.notify_all();
        (bytes, finished)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.drained.notify_all();
    }

    fn exit(&self) {
        self.exited.store(true, Ordering::Relaxed);
        self.drained.notify_all();
    }

    fn set_eof(&self) {
        self.out.lock().unwrap_or_else(|p| p.into_inner()).eof = true;
    }
}

pub struct SpawnOptions {
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    /// Explicit shell/program. `None` picks the platform default.
    pub shell: Option<String>,
}

struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    /// Its own lock (plan-12): a write blocks when the child stops reading
    /// its pipe, and that must not hold `panes` against every other pane.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// The shell's pid; agent detection walks its descendants.
    pid: Option<u32>,
    shared: Arc<Shared>,
}

#[derive(Default)]
pub struct PtyRegistry {
    next_id: AtomicU32,
    panes: Mutex<HashMap<u32, PtyHandle>>,
    /// Outboxes outlive their handles: a shell that exits at once still has
    /// output the frontend has not fetched. An entry leaves when the
    /// frontend has taken everything after EOF, or when it kills the pane.
    outboxes: Mutex<HashMap<u32, Arc<Shared>>>,
}

fn no_pty(id: u32) -> String {
    format!("no pty {id}")
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Whether a bare program name resolves on PATH. On Windows `pwsh` finds
/// `pwsh.exe`, as CreateProcess would.
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|d| {
        d.join(program).is_file() || (cfg!(windows) && d.join(format!("{program}.exe")).is_file())
    })
}

/// Platform default shell. Windows: `pwsh.exe` if it is on PATH, else
/// `powershell.exe`. Elsewhere: `$SHELL`, else `/bin/sh`.
pub fn default_shell() -> String {
    if cfg!(windows) {
        if on_path("pwsh.exe") {
            "pwsh.exe".into()
        } else {
            "powershell.exe".into()
        }
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

/// A shell that cannot be found is named plainly (plan-16), not as the
/// `CreateProcessW` debug string portable-pty would produce.
fn check_shell(shell: &str) -> Result<(), String> {
    let is_path = shell.contains(['\\', '/']);
    if is_path && !std::path::Path::new(shell).is_file() {
        return Err(format!("{shell}: not found"));
    }
    if !is_path && !on_path(shell) {
        return Err(format!("{shell} is not on PATH"));
    }
    Ok(())
}

impl PtyRegistry {
    /// Spawn a shell. `on_data_ready` runs on the reader thread when the
    /// outbox goes from empty to non-empty (the frontend then `take`s until it
    /// is empty again); `on_exit` runs exactly once, after the handle has been
    /// removed from the registry, with the pane id and exit code (`None` if it
    /// could not be read).
    pub fn spawn(
        self: &Arc<Self>,
        opts: SpawnOptions,
        mut on_data_ready: impl FnMut(u32) + Send + 'static,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<u32, String> {
        // Windows shells silently fall back to the profile directory when the
        // cwd is gone (a restored pane whose project was deleted); fail loudly
        // instead so the pane shows its error footer.
        if !std::path::Path::new(&opts.cwd).is_dir() {
            return Err(format!("no such directory: {}", opts.cwd));
        }
        let shell = opts.shell.unwrap_or_else(default_shell);
        check_shell(&shell)?;
        let pair = native_pty_system()
            .openpty(size(opts.cols, opts.rows))
            .map_err(|e| e.to_string())?;

        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&opts.cwd);
        // Ignored by ConPTY; required on unix so the shell knows it has a real terminal.
        cmd.env("TERM", "xterm-256color");
        // Tells the `veal` launchers not to exit this shell after launching.
        cmd.env("VEAL_PANE", "1");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn {shell}: {e}"))?;
        // The slave handles belong to the child now.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        let pid = child.process_id();

        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let shared = Arc::new(Shared::default());
        self.outboxes.lock().unwrap().insert(id, Arc::clone(&shared));
        self.panes.lock().unwrap().insert(
            id,
            PtyHandle {
                master: pair.master,
                writer: Arc::new(Mutex::new(writer)),
                killer,
                pid,
                shared: Arc::clone(&shared),
            },
        );

        // Either thread failing to start (plan-16) must not leave a pane
        // registered with a live child and nobody to reap it.
        let reader_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name(format!("pty-{id}-read"))
            .spawn(move || {
                let mut buf = vec![0u8; READ_BUF];
                let mut bytes = 0usize;
                let mut window = std::time::Instant::now();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            // Always read to EOF: on Windows ClosePseudoConsole
                            // waits for the pipe to drain. `push` discards
                            // once the pane is closed.
                            if reader_shared.push(&buf[..n]) {
                                on_data_ready(id);
                            }
                            if cfg!(debug_assertions) {
                                bytes += n;
                                if window.elapsed().as_secs() >= 1 {
                                    eprintln!("[pty {id}] {} KB/s read", bytes / 1024);
                                    bytes = 0;
                                    window = std::time::Instant::now();
                                }
                            }
                        }
                    }
                }
                reader_shared.set_eof();
            })
            .map_err(|e| {
                self.undo_spawn(id);
                format!("spawn reader thread: {e}")
            })?;

        let registry = Arc::clone(self);
        thread::Builder::new()
            .name(format!("pty-{id}-wait"))
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                // Release a reader waiting at the cap before the drop below:
                // ClosePseudoConsole needs the reader to reach EOF.
                shared.exit();
                // Drop the handle outside the lock: on Windows this is
                // ClosePseudoConsole, which blocks until output is drained.
                let handle = registry.panes.lock().unwrap().remove(&id);
                drop(handle);
                on_exit(id, code);
            })
            .map_err(|e| {
                self.undo_spawn(id);
                format!("spawn waiter thread: {e}")
            })?;

        Ok(id)
    }

    /// Takes a half-spawned pane back: kill the child, forget the handle
    /// and the outbox. A reader that did start ends at EOF once the master
    /// drops with the handle.
    fn undo_spawn(&self, id: u32) {
        if let Some(s) = self.outboxes.lock().unwrap().remove(&id) {
            s.close();
        }
        if let Some(mut h) = self.panes.lock().unwrap().remove(&id) {
            let _ = h.killer.kill();
        }
    }

    /// Everything read since the last take. Empty for an unknown id: the
    /// pane is gone and fully drained, which is not an error for a drain
    /// loop. The outbox is dropped once EOF has been reached and taken.
    pub fn take(&self, id: u32) -> Vec<u8> {
        let shared = {
            let outboxes = self.outboxes.lock().unwrap();
            match outboxes.get(&id) {
                Some(s) => Arc::clone(s),
                None => return Vec::new(),
            }
        };
        let (bytes, finished) = shared.take();
        if finished && bytes.is_empty() {
            self.outboxes.lock().unwrap().remove(&id);
        }
        bytes
    }

    /// Writes to the child. The registry lock is held only to find the
    /// writer; the I/O itself happens under the pane's own lock.
    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let writer = {
            let panes = self.panes.lock().unwrap();
            Arc::clone(&panes.get(&id).ok_or_else(|| no_pty(id))?.writer)
        };
        let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
        w.write_all(data).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let panes = self.panes.lock().unwrap();
        let h = panes.get(&id).ok_or_else(|| no_pty(id))?;
        h.master.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    /// Terminate the child. Idempotent: an unknown id is not an error, because
    /// the waiter thread may already have removed it. Cleanup and the exit
    /// callback still happen on the waiter thread.
    /// The frontend is done with the pane: its outbox is dropped and the
    /// reader discards from here on, so a full outbox can never hold up
    /// the child's teardown.
    pub fn kill(&self, id: u32) -> Result<(), String> {
        if let Some(s) = self.outboxes.lock().unwrap().remove(&id) {
            s.close();
        }
        let mut panes = self.panes.lock().unwrap();
        if let Some(h) = panes.get_mut(&id) {
            h.killer.kill().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Kill every registered child. Used on window close so no shells or
    /// agents outlive the app, and at boot (plan-16) so a reloaded page does
    /// not leave its predecessor's shells running. Nothing will fetch the
    /// outboxes, so they go too.
    pub fn kill_all(&self) {
        let outboxes: Vec<_> = self.outboxes.lock().unwrap().drain().map(|(_, s)| s).collect();
        for s in outboxes {
            s.close();
        }
        let mut panes = self.panes.lock().unwrap();
        for h in panes.values_mut() {
            h.shared.close();
            let _ = h.killer.kill();
        }
    }

    /// `(pane id, shell pid)` for every live pane whose pid is known.
    pub fn pids(&self) -> Vec<(u32, u32)> {
        let panes = self.panes.lock().unwrap();
        let mut v: Vec<_> = panes.iter().filter_map(|(id, h)| h.pid.map(|p| (*id, p))).collect();
        v.sort_unstable();
        v
    }

    #[cfg(test)]
    pub fn contains(&self, id: u32) -> bool {
        self.panes.lock().unwrap().contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn echo_shell() -> String {
        if cfg!(windows) {
            "cmd.exe".into()
        } else {
            "/bin/sh".into()
        }
    }

    /// Drain the pane's outbox into `acc` until `pred` matches or `timeout`
    /// passes. `ready` carries the data-ready signals; a take happens after
    /// each one, and on a short tick regardless.
    fn read_until(
        registry: &PtyRegistry,
        id: u32,
        ready: &mpsc::Receiver<u32>,
        acc: &mut Vec<u8>,
        timeout: Duration,
        pred: impl Fn(&[u8]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            acc.extend(registry.take(id));
            if pred(acc) {
                return true;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let _ = ready.recv_timeout(left.min(Duration::from_millis(50)));
        }
    }

    // ---- Outbox / Shared: the pull protocol without a PTY ----

    #[test]
    fn push_signals_once_until_taken() {
        let s = Shared::default();
        assert!(s.push(b"a"), "first push after empty signals");
        assert!(!s.push(b"b"), "outbox already announced");
        assert_eq!(s.take(), (b"ab".to_vec(), false));
        assert!(s.push(b"c"), "take cleared the signal");
        assert_eq!(s.take(), (b"c".to_vec(), false));
        assert_eq!(s.take(), (Vec::new(), false));
    }

    #[test]
    fn push_blocks_at_cap_until_take() {
        let s = Arc::new(Shared::default());
        s.push(&vec![0u8; OUTBOX_CAP]);
        let (tx, rx) = mpsc::channel();
        let s2 = Arc::clone(&s);
        thread::spawn(move || {
            let signalled = s2.push(b"late");
            let _ = tx.send(signalled);
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "push must block while the outbox is full"
        );
        let (bytes, _) = s.take();
        assert_eq!(bytes.len(), OUTBOX_CAP);
        // Released: the blocked push lands and, the outbox having been
        // emptied by the take, signals again.
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(true));
        assert_eq!(s.take().0, b"late".to_vec());
    }

    #[test]
    fn close_releases_a_blocked_push_and_discards() {
        let s = Arc::new(Shared::default());
        s.push(&vec![0u8; OUTBOX_CAP]);
        let (tx, rx) = mpsc::channel();
        let s2 = Arc::clone(&s);
        thread::spawn(move || {
            let _ = tx.send(s2.push(b"x"));
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        s.close();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(false));
        assert!(!s.push(b"y"), "closed: nothing is buffered");
        assert_eq!(s.take().0.len(), OUTBOX_CAP, "what was buffered before close stays");
    }

    #[test]
    fn exit_releases_a_blocked_push_without_closing() {
        let s = Arc::new(Shared::default());
        s.push(&vec![0u8; OUTBOX_CAP]);
        let s2 = Arc::clone(&s);
        let t = thread::spawn(move || s2.push(b"x"));
        s.exit();
        assert!(!t.join().unwrap(), "full at exit: the bytes are dropped");
        s.take();
        assert!(s.push(b"after"), "below the cap, an exited pane still buffers");
        s.set_eof();
        assert_eq!(s.take(), (b"after".to_vec(), true));
    }

    #[test]
    fn take_of_unknown_id_is_empty() {
        let registry = PtyRegistry::default();
        assert!(registry.take(9).is_empty());
    }

    /// Spawn a shell, type an echo, check the bytes come back through
    /// `take`, then that `on_exit` fires and the handle is gone.
    #[test]
    fn spawn_echo_reads_output_and_cleans_up() {
        let registry = Arc::new(PtyRegistry::default());
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        let (exit_tx, exit_rx) = mpsc::channel::<Option<i32>>();

        let id = registry
            .spawn(
                SpawnOptions {
                    cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                    cols: 80,
                    rows: 24,
                    shell: Some(echo_shell()),
                },
                move |id| {
                    let _ = ready_tx.send(id);
                },
                move |_id, code| {
                    let _ = exit_tx.send(code);
                },
            )
            .expect("spawn");
        assert!(registry.contains(id));

        let mut out = Vec::new();
        if cfg!(windows) {
            // ConPTY asks the terminal where the cursor is (CSI 6n) before it
            // runs the shell and blocks until it gets an answer. xterm.js
            // replies in the app; here we have to play the terminal.
            let asked = read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
                b.windows(4).any(|w| w == b"\x1b[6n")
            });
            assert!(
                asked,
                "no DSR from ConPTY; got: {:?}",
                String::from_utf8_lossy(&out)
            );
            registry.write(id, b"\x1b[1;1R").unwrap();
        }

        // Type only once the shell is at its prompt; input sent while it is
        // still starting up can be dropped.
        let prompted = read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
            b.contains(&b'>') || b.contains(&b'$')
        });
        assert!(
            prompted,
            "no prompt; got: {:?}",
            String::from_utf8_lossy(&out)
        );

        registry.write(id, b"echo veal-ok\r\n").unwrap();
        let echoed = read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
            // Look past the input echo for the command's own output line.
            String::from_utf8_lossy(b).matches("veal-ok").count() >= 2
        });
        assert!(
            echoed,
            "no echo output; got: {:?}",
            String::from_utf8_lossy(&out)
        );

        registry.write(id, b"exit\r\n").unwrap();
        let code = exit_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("pty-exit within 10s");
        assert_eq!(code, Some(0));
        assert!(!registry.contains(id), "handle removed after exit");

        // The outbox outlives the handle until the tail has been taken.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            registry.take(id);
            if !registry.outboxes.lock().unwrap().contains_key(&id) {
                break;
            }
            assert!(Instant::now() < deadline, "outbox not released after EOF");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn missing_cwd_is_an_error_not_a_fallback() {
        let registry = Arc::new(PtyRegistry::default());
        let cwd = std::env::temp_dir()
            .join(format!("veal-pty-missing-{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let err = registry
            .spawn(
                SpawnOptions {
                    cwd: cwd.clone(),
                    cols: 80,
                    rows: 24,
                    shell: Some(echo_shell()),
                },
                |_| {},
                |_, _| {},
            )
            .unwrap_err();
        assert_eq!(err, format!("no such directory: {cwd}"));
        assert!(!registry.contains(1), "nothing should be registered");
    }

    /// Plan-16: the footer reads `failed to start shell: X is not on PATH`.
    #[test]
    fn missing_shell_is_named_plainly() {
        let registry = Arc::new(PtyRegistry::default());
        let cwd = std::env::temp_dir().to_string_lossy().into_owned();
        let spawn = |shell: &str| {
            registry.spawn(
                SpawnOptions { cwd: cwd.clone(), cols: 80, rows: 24, shell: Some(shell.into()) },
                |_| {},
                |_, _| {},
            )
        };
        assert_eq!(spawn("zz-no-such-shell.exe"), Err("zz-no-such-shell.exe is not on PATH".into()));
        let as_path = std::env::temp_dir().join("zz-no-such-shell.exe").to_string_lossy().into_owned();
        assert_eq!(spawn(&as_path), Err(format!("{as_path}: not found")));
        assert!(!registry.contains(1), "nothing should be registered");
        assert!(registry.pids().is_empty());
    }

    #[test]
    fn unknown_id_errors() {
        let registry = Arc::new(PtyRegistry::default());
        assert_eq!(registry.write(7, b"x"), Err("no pty 7".into()));
        assert_eq!(registry.resize(7, 80, 24), Err("no pty 7".into()));
        assert_eq!(registry.kill(7), Ok(()));
    }
}
