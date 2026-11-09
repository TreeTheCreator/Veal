//! PTY plumbing: one real shell process per pane, driven by portable-pty.
//!
//! Design: no async runtime. portable-pty is blocking, so each pane owns
//! `std::thread`s: a reader that pumps raw bytes into its `Session`, a waiter
//! that blocks on `child.wait()`, and one that writes query answers. The waiter
//! is what tears the pane down: on Windows the ConPTY reader only sees EOF once
//! the pseudoconsole is closed, which happens when we drop the master, so "child
//! exited" has to be observed independently of the byte stream.
//!
//! Output is pulled, not pushed (plan-15): the reader feeds the session, which
//! signals a client once per idle-to-busy edge, and the client drains with
//! `take` at its own pace. Reading never blocks (plan-19): the child is limited
//! only by the emulator, and a client that falls behind gets a snapshot.
//!
//! The registry is Tauri-agnostic (callbacks instead of channels/events) so it
//! can be exercised from `cargo test` without a webview.

use crate::session::{Attached, ClientId, Notify, Session};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Bytes per `read()` on the PTY master.
const READ_BUF: usize = 16 * 1024;

/// The client `spawn_with_env` attaches before the shell starts, the one
/// `take` reads for. It answers queries, as xterm always has.
const FIRST_CLIENT: ClientId = 0;

/// Sessions that may run at once. Mirrors `PANE_CAP` in src/layout/tree.ts,
/// and a test holds the two together: no client can make a fifth (plan-19).
pub const MAX_SESSIONS: usize = 4;

/// How long an answer a client left owing waits for that client's own answer,
/// which a page reload can leave in flight.
const OWED_GRACE: Duration = Duration::from_millis(250);

/// Runs with the pane id when a client's queue goes from empty to non-empty.
pub type DataReady = Arc<dyn Fn(u32) + Send + Sync>;

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
    session: Arc<Session>,
}

struct Entry {
    session: Arc<Session>,
    /// The name a client knows this session by, stable across page reloads.
    key: String,
    /// Opened by key: its pane shows the final screen and code on every reload,
    /// so the tombstone stays until the pane closes (`kill`) or has no pane (`forget`).
    named: bool,
    cwd: String,
    /// The last answer from agent detection, for a client attaching later:
    /// the poller itself only reports changes.
    agent: Option<String>,
}

/// What `open` hands a client.
#[derive(Debug, PartialEq)]
pub struct Opened {
    pub id: u32,
    pub client: ClientId,
    /// Set if the child had already exited: a tombstone, showing its last screen.
    pub exit: Option<Option<i32>>,
    pub agent: Option<String>,
}

pub use crate::protocol::SessionInfo;

#[derive(Default)]
pub struct PtyRegistry {
    next_id: AtomicU32,
    panes: Mutex<HashMap<u32, PtyHandle>>,
    /// Held across `open`, so two opens of one key cannot both spawn.
    opening: Mutex<()>,
    /// Sessions outlive their handles: a shell that exits at once still has
    /// output nobody has fetched. An entry leaves when its last client has
    /// taken everything after EOF, or when it is killed or forgotten.
    sessions: Mutex<HashMap<u32, Entry>>,
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
    /// Spawn a shell with no extra environment.
    #[cfg(test)]
    pub fn spawn(
        self: &Arc<Self>,
        opts: SpawnOptions,
        on_data_ready: impl Fn(u32) + Send + Sync + 'static,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<u32, String> {
        self.spawn_with_env(opts, Vec::new(), on_data_ready, on_exit)
    }

    /// Spawn a shell with a client attached from its first byte. `on_data_ready`
    /// runs when that client's queue goes from empty to non-empty (it then
    /// `take`s until empty); `on_exit` runs exactly once, after the handle has
    /// been removed from the registry, with the pane id and exit code (`None`
    /// if it could not be read).
    /// `env` is set on top of the inherited environment: the proxy
    /// variables `pty_spawn` resolved from the proxy mode (plan-18).
    pub fn spawn_with_env(
        self: &Arc<Self>,
        opts: SpawnOptions,
        env: Vec<(String, String)>,
        on_data_ready: impl Fn(u32) + Send + Sync + 'static,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<u32, String> {
        self.start(None, opts, env, Some(Arc::new(on_data_ready)), on_exit)
    }

    /// Spawn a shell with nobody attached: the session answers its queries
    /// until a client attaches.
    pub fn spawn_detached(
        self: &Arc<Self>,
        opts: SpawnOptions,
        env: Vec<(String, String)>,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<u32, String> {
        self.start(None, opts, env, None, on_exit)
    }

    /// Attach to the session called `key`, if there is one, sized as the client
    /// is. Its first take is a snapshot. This is the cheap half of `open`: it
    /// lets a caller skip work that only a spawn needs.
    pub fn attach_key(&self, key: &str, cols: u16, rows: u16, on_data_ready: DataReady) -> Option<Opened> {
        let (id, session, agent) = {
            let sessions = self.sessions.lock().unwrap();
            let (id, e) = sessions.iter().find(|(_, e)| e.key == key)?;
            (*id, Arc::clone(&e.session), e.agent.clone())
        };
        if let Some(h) = self.panes.lock().unwrap().get(&id) {
            // A repaint is not free, and the same size would not need one.
            if h.master.get_size().map_or(true, |s| s.cols != cols || s.rows != rows) {
                session.resize(cols, rows);
                let _ = h.master.resize(size(cols, rows));
            }
        }
        let attached = session.attach(true, Arc::new(move || on_data_ready(id)));
        self.send_owed(id, &session);
        Some(Opened { id, client: attached.client, exit: attached.exit, agent })
    }

    /// Attach to the session called `key`, or spawn it. Idempotent, so React's
    /// double effect and two racing panes cannot make a second shell.
    pub fn open(
        self: &Arc<Self>,
        key: &str,
        opts: SpawnOptions,
        env: Vec<(String, String)>,
        on_data_ready: DataReady,
        on_exit: impl FnOnce(u32, Option<i32>) + Send + 'static,
    ) -> Result<Opened, String> {
        let _one_at_a_time = self.opening.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(opened) = self.attach_key(key, opts.cols, opts.rows, Arc::clone(&on_data_ready)) {
            return Ok(opened);
        }
        let live = self.sessions.lock().unwrap().values().filter(|e| e.session.exit().is_none()).count();
        if live >= MAX_SESSIONS {
            return Err(format!("already running {MAX_SESSIONS} terminals"));
        }
        let id = self.start(Some(key.to_string()), opts, env, Some(on_data_ready), on_exit)?;
        Ok(Opened { id, client: FIRST_CLIENT, exit: None, agent: None })
    }

    /// Every session, running or exited, oldest first.
    pub fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().unwrap();
        let mut rows: Vec<_> = sessions.iter().collect();
        rows.sort_unstable_by_key(|(id, _)| **id);
        rows.into_iter()
            .map(|(_, e)| {
                let exit = e.session.exit();
                SessionInfo { key: e.key.clone(), cwd: e.cwd.clone(), exited: exit.is_some(), code: exit.flatten() }
            })
            .collect()
    }

    fn id_of(&self, key: &str) -> Option<u32> {
        self.sessions.lock().unwrap().iter().find(|(_, e)| e.key == key).map(|(id, _)| *id)
    }

    /// `kill`, for a client that knows the name. A key that is gone is fine.
    pub fn kill_key(&self, key: &str) -> Result<(), String> {
        self.id_of(key).map_or(Ok(()), |id| self.kill(id))
    }

    /// `forget`, for a client that knows the name.
    pub fn forget_key(&self, key: &str) {
        if let Some(id) = self.id_of(key) {
            self.forget(id);
        }
    }

    /// The page went away with no chance to say so: every client is gone, and
    /// every shell keeps running.
    pub fn detach_all(&self) {
        let sessions: Vec<_> = self.sessions.lock().unwrap().iter().map(|(id, e)| (*id, Arc::clone(&e.session))).collect();
        for (id, session) in sessions {
            session.detach_all();
            self.send_owed(id, &session);
        }
    }

    /// Off this thread, like every other answer: the child may not be reading.
    /// After `OWED_GRACE`, in which the old page's own answer may still land.
    fn send_owed(&self, id: u32, session: &Arc<Session>) {
        if !session.owes() {
            return;
        }
        let writer = self.panes.lock().unwrap().get(&id).map(|h| Arc::clone(&h.writer));
        let Some(writer) = writer else { return };
        let session = Arc::clone(session);
        let _ = thread::Builder::new().name(format!("pty-{id}-owed")).spawn(move || {
            thread::sleep(OWED_GRACE);
            let owed = session.take_owed();
            if !owed.is_empty() {
                let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
                let _ = w.write_all(&owed).and_then(|()| w.flush());
            }
        });
    }

    /// Agent detection's latest answer for a pane.
    pub fn note_agent(&self, id: u32, agent: Option<String>) {
        if let Some(e) = self.sessions.lock().unwrap().get_mut(&id) {
            e.agent = agent;
        }
    }

    fn start(
        self: &Arc<Self>,
        key: Option<String>,
        opts: SpawnOptions,
        env: Vec<(String, String)>,
        first_client: Option<DataReady>,
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
        // For a VPN that exposes a local HTTP proxy rather than routing the
        // whole machine: proxy-aware CLIs (curl, git, npm, pip) pick these up.
        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn {shell}: {e}"))?;
        // The slave handles belong to the child now.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));
        let killer = child.clone_killer();
        let pid = child.process_id();

        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let session = Arc::new(Session::new(opts.cols, opts.rows));
        if let Some(notify) = first_client {
            session.first_client(Arc::new(move || notify(id)));
        }
        self.sessions.lock().unwrap().insert(
            id,
            Entry {
                session: Arc::clone(&session),
                named: key.is_some(),
                key: key.unwrap_or_else(|| format!("pane-{id}")),
                cwd: opts.cwd.clone(),
                agent: None,
            },
        );
        // Holds the writer only weakly, so the handle still decides when it closes.
        let reply_writer = Arc::downgrade(&writer);
        self.panes.lock().unwrap().insert(
            id,
            PtyHandle {
                master: pair.master,
                writer,
                killer,
                pid,
                session: Arc::clone(&session),
            },
        );

        // Query answers leave the reader's thread: a child that has stopped
        // reading would block the write, and the reader must never stop. Ends
        // when the reader drops the sender.
        let (replies, reply_queue) = mpsc::channel::<Vec<u8>>();
        // Any thread failing to start (plan-16) must not leave a pane
        // registered with a live child and nobody to reap it.
        thread::Builder::new()
            .name(format!("pty-{id}-reply"))
            .spawn(move || {
                for reply in reply_queue {
                    let Some(writer) = reply_writer.upgrade() else { break };
                    let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
                    let _ = w.write_all(&reply).and_then(|()| w.flush());
                }
            })
            .map_err(|e| {
                self.undo_spawn(id);
                format!("spawn reply thread: {e}")
            })?;

        let reader_session = Arc::clone(&session);
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
                            // waits for the pipe to drain. `feed` discards
                            // once the session is closed.
                            let reply = reader_session.feed(&buf[..n]);
                            if !reply.is_empty() {
                                let _ = replies.send(reply);
                            }
                            if cfg!(debug_assertions) {
                                bytes += n;
                                if window.elapsed().as_secs() >= 1 {
                                    crate::vlog!("[pty {id}] {} KB/s read", bytes / 1024);
                                    bytes = 0;
                                    window = std::time::Instant::now();
                                }
                            }
                        }
                    }
                }
                reader_session.set_eof();
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
                session.set_exit(code);
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
    /// and the session. A reader that did start ends at EOF once the master
    /// drops with the handle.
    fn undo_spawn(&self, id: u32) {
        if let Some(e) = self.sessions.lock().unwrap().remove(&id) {
            e.session.close();
        }
        if let Some(mut h) = self.panes.lock().unwrap().remove(&id) {
            let _ = h.killer.kill();
        }
    }

    fn session(&self, id: u32) -> Option<Arc<Session>> {
        self.sessions.lock().unwrap().get(&id).map(|e| Arc::clone(&e.session))
    }

    /// Everything since the first client's last take.
    pub fn take(&self, id: u32) -> Vec<u8> {
        self.take_from(id, FIRST_CLIENT)
    }

    /// Everything since this client's last take, or a snapshot if it owes one.
    /// Empty for an unknown id or client: it is gone and fully drained, which
    /// is not an error for a drain loop. A session with no key goes once EOF has
    /// been reached and its last client has taken it all.
    pub fn take_from(&self, id: u32, client: ClientId) -> Vec<u8> {
        let Some(session) = self.session(id) else { return Vec::new() };
        let taken = session.take(client);
        if taken.finished {
            let mut sessions = self.sessions.lock().unwrap();
            if sessions.get(&id).is_some_and(|e| !e.named) {
                sessions.remove(&id);
            }
        }
        taken.bytes
    }

    /// A client's first take is a snapshot, so it picks up exactly where the
    /// stream is. On an exited session this is how the tombstone gets seen.
    pub fn attach(&self, id: u32, responder: bool, notify: Notify) -> Result<Attached, String> {
        let session = self.session(id).ok_or_else(|| no_pty(id))?;
        let attached = session.attach(responder, notify);
        self.send_owed(id, &session);
        Ok(attached)
    }

    /// The shell keeps running. Idempotent, like `kill`.
    pub fn detach(&self, id: u32, client: ClientId) {
        if let Some(session) = self.session(id) {
            session.detach(client);
            self.send_owed(id, &session);
        }
    }

    /// Drop a tombstone nobody is going to attach to. A live session has to be
    /// killed; this leaves it alone.
    pub fn forget(&self, id: u32) {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(&id).is_some_and(|e| e.session.exit().is_some()) {
            if let Some(e) = sessions.remove(&id) {
                e.session.close();
            }
        }
    }

    /// Writes to the child. The registry lock is held only to find the
    /// writer; the I/O itself happens under the pane's own lock.
    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let writer = {
            let panes = self.panes.lock().unwrap();
            let h = panes.get(&id).ok_or_else(|| no_pty(id))?;
            h.session.note_input(data);
            Arc::clone(&h.writer)
        };
        let mut w = writer.lock().unwrap_or_else(|p| p.into_inner());
        w.write_all(data).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    /// The emulator goes first: whatever the child draws after the resize has
    /// to land on a grid of the new size.
    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let panes = self.panes.lock().unwrap();
        let h = panes.get(&id).ok_or_else(|| no_pty(id))?;
        h.session.resize(cols, rows);
        h.master.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    /// Terminate the child. Idempotent: an unknown id is not an error, because
    /// the waiter thread may already have removed it. Cleanup and the exit
    /// callback still happen on the waiter thread.
    /// Nobody wants the pane any more, so the session goes now and the reader
    /// discards from here on.
    pub fn kill(&self, id: u32) -> Result<(), String> {
        if let Some(e) = self.sessions.lock().unwrap().remove(&id) {
            e.session.close();
        }
        let mut panes = self.panes.lock().unwrap();
        if let Some(h) = panes.get_mut(&id) {
            // Not checked: portable-pty 0.9's Windows killer returns `Err` when
            // TerminateProcess succeeds and `Ok` when it fails.
            let _ = h.killer.kill();
        }
        Ok(())
    }

    /// Kill every registered child. Used on window close so no shells or
    /// agents outlive the app. Nothing will fetch from the sessions, so they
    /// go too.
    pub fn kill_all(&self) {
        let sessions: Vec<_> = self.sessions.lock().unwrap().drain().map(|(_, e)| e).collect();
        for e in sessions {
            e.session.close();
        }
        let mut panes = self.panes.lock().unwrap();
        for h in panes.values_mut() {
            h.session.close();
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

    /// Drain the pane's first client into `acc` until `pred` matches or `timeout`
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

    #[test]
    fn take_of_unknown_id_is_empty() {
        let registry = PtyRegistry::default();
        assert!(registry.take(9).is_empty());
    }

    fn temp_opts() -> SpawnOptions {
        SpawnOptions {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cols: 80,
            rows: 24,
            shell: Some(echo_shell()),
        }
    }

    /// Drains a client until `pred` matches or `timeout` passes.
    fn drain_until(
        registry: &PtyRegistry,
        id: u32,
        client: ClientId,
        acc: &mut Vec<u8>,
        timeout: Duration,
        pred: impl Fn(&str) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            acc.extend(registry.take_from(id, client));
            if pred(&String::from_utf8_lossy(acc)) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Nobody plays terminal in this test. ConPTY's opening query stalls the
    /// shell unless the session answers it, so a prompt proves that it did.
    #[test]
    fn a_shell_starts_with_nobody_attached_and_restores_on_attach() {
        let registry = Arc::new(PtyRegistry::default());
        let (exit_tx, exit_rx) = mpsc::channel::<Option<i32>>();
        let id = registry
            .spawn_detached(temp_opts(), Vec::new(), move |_, code| {
                let _ = exit_tx.send(code);
            })
            .expect("spawn");
        thread::sleep(Duration::from_millis(1500));

        let attached = registry.attach(id, true, Arc::new(|| {})).expect("attach");
        assert_eq!(attached.exit, None);
        let mut out = Vec::new();
        let prompted = drain_until(&registry, id, attached.client, &mut out, Duration::from_secs(10), |s| {
            s.contains('>') || s.contains('$')
        });
        assert!(prompted, "no prompt in the snapshot: {:?}", String::from_utf8_lossy(&out));
        assert!(out.starts_with(b"\x1bc"), "the first bytes are a snapshot");

        registry.write(id, b"echo veal-ok\r\n").unwrap();
        let echoed = drain_until(&registry, id, attached.client, &mut out, Duration::from_secs(10), |s| {
            s.matches("veal-ok").count() >= 2
        });
        assert!(echoed, "no echo after attach: {:?}", String::from_utf8_lossy(&out));

        registry.write(id, b"exit\r\n").unwrap();
        assert_eq!(exit_rx.recv_timeout(Duration::from_secs(10)), Ok(Some(0)));
        let deadline = Instant::now() + Duration::from_secs(5);
        while registry.session(id).is_some() {
            registry.take_from(id, attached.client);
            assert!(Instant::now() < deadline, "session not released after EOF");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn an_exit_with_nobody_attached_stays_until_seen() {
        let registry = Arc::new(PtyRegistry::default());
        let (exit_tx, exit_rx) = mpsc::channel::<Option<i32>>();
        let id = registry
            .spawn_detached(temp_opts(), Vec::new(), move |_, code| {
                let _ = exit_tx.send(code);
            })
            .expect("spawn");
        thread::sleep(Duration::from_millis(1500));
        let cmd: &[u8] = if cfg!(windows) { b"exit 3\r\n" } else { b"exit 3\n" };
        registry.write(id, cmd).unwrap();
        assert_eq!(exit_rx.recv_timeout(Duration::from_secs(10)), Ok(Some(3)));
        assert!(registry.session(id).is_some(), "a tombstone");

        registry.forget(id + 1);
        let attached = registry.attach(id, true, Arc::new(|| {})).expect("attach");
        assert_eq!(attached.exit, Some(Some(3)));
        let mut out = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while registry.session(id).is_some() {
            out.extend(registry.take_from(id, attached.client));
            assert!(Instant::now() < deadline, "tombstone not released once seen");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(String::from_utf8_lossy(&out).contains("exit 3"), "the final screen: {:?}", String::from_utf8_lossy(&out));
    }

    #[test]
    fn forget_drops_a_tombstone_and_leaves_a_live_session() {
        let registry = Arc::new(PtyRegistry::default());
        let (exit_tx, exit_rx) = mpsc::channel::<Option<i32>>();
        let id = registry
            .spawn_detached(temp_opts(), Vec::new(), move |_, code| {
                let _ = exit_tx.send(code);
            })
            .expect("spawn");
        registry.forget(id);
        assert!(registry.session(id).is_some(), "still running");
        assert!(registry.contains(id));
        thread::sleep(Duration::from_millis(1500));
        let cmd: &[u8] = if cfg!(windows) { b"exit\r\n" } else { b"exit\n" };
        registry.write(id, cmd).unwrap();
        exit_rx.recv_timeout(Duration::from_secs(10)).expect("exit");
        registry.forget(id);
        assert!(registry.session(id).is_none());
        assert_eq!(registry.attach(id, true, Arc::new(|| {})).err(), Some(format!("no pty {id}")));
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

        // The session outlives the handle until the tail has been taken.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            registry.take(id);
            if !registry.session(id).is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "session not released after EOF");
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

    /// The proxy setting reaches the shell's environment, by the same path
    /// `pty_spawn` takes (plan-18).
    #[test]
    fn proxy_is_exported_to_the_shell() {
        use crate::proxy::{proxy_env, resolve, Lookup, Mode};
        let resolved = resolve(Mode::Manual, Some("15236"), || Lookup::None, |_| true);
        let registry = Arc::new(PtyRegistry::default());
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        let id = registry
            .spawn_with_env(
                SpawnOptions {
                    cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                    cols: 80,
                    rows: 24,
                    shell: Some(echo_shell()),
                },
                proxy_env(&resolved, None),
                move |id| {
                    let _ = ready_tx.send(id);
                },
                |_, _| {},
            )
            .expect("spawn");

        let mut out = Vec::new();
        if cfg!(windows) {
            read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
                b.windows(4).any(|w| w == b"\x1b[6n")
            });
            registry.write(id, b"\x1b[1;1R").unwrap();
        }
        read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
            b.contains(&b'>') || b.contains(&b'$')
        });
        // The input echo shows the variable name; only the expansion has the port.
        let cmd: &[u8] = if cfg!(windows) { b"echo %https_proxy%\r\n" } else { b"echo $https_proxy\n" };
        registry.write(id, cmd).unwrap();
        let seen = read_until(&registry, id, &ready_rx, &mut out, Duration::from_secs(10), |b| {
            String::from_utf8_lossy(b).contains("127.0.0.1:15236")
        });
        assert!(seen, "proxy not in env; got: {:?}", String::from_utf8_lossy(&out));
        registry.kill(id).ok();
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

    fn open_key(registry: &Arc<PtyRegistry>, key: &str) -> Result<Opened, String> {
        registry.open(key, temp_opts(), Vec::new(), Arc::new(|_| {}), |_, _| {})
    }

    #[test]
    fn open_spawns_once_per_key_and_attaches_after_that() {
        let registry = Arc::new(PtyRegistry::default());
        let first = open_key(&registry, "key-a").expect("spawn");
        assert_eq!(first.client, FIRST_CLIENT);
        let again = open_key(&registry, "key-a").expect("attach");
        assert_eq!(again.id, first.id, "the same shell");
        assert_ne!(again.client, first.client);
        assert_eq!(registry.pids().len(), 1);
        assert!(registry.take_from(again.id, again.client).starts_with(b"\x1bc"), "attach starts with a snapshot");
        let other = open_key(&registry, "key-b").expect("spawn");
        assert_ne!(other.id, first.id);
        assert_eq!(registry.list().iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["key-a", "key-b"]);
        registry.kill_all();
    }

    #[test]
    fn opens_racing_on_one_key_make_one_shell() {
        let registry = Arc::new(PtyRegistry::default());
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let r = Arc::clone(&registry);
                thread::spawn(move || open_key(&r, "racing").expect("open").id)
            })
            .collect();
        let ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(ids.iter().all(|id| *id == ids[0]), "{ids:?}");
        assert_eq!(registry.pids().len(), 1);
        registry.kill_all();
    }

    #[test]
    fn attach_resizes_to_the_clients_size() {
        let registry = Arc::new(PtyRegistry::default());
        let id = open_key(&registry, "sized").expect("spawn").id;
        let current = || registry.panes.lock().unwrap().get(&id).unwrap().master.get_size().unwrap();
        registry.attach_key("sized", 100, 30, Arc::new(|_| {})).expect("attach");
        assert_eq!((current().cols, current().rows), (100, 30));
        assert!(registry.attach_key("nope", 100, 30, Arc::new(|_| {})).is_none());
        registry.kill_all();
    }

    #[test]
    fn a_fifth_session_is_refused_and_an_attach_is_not() {
        let registry = Arc::new(PtyRegistry::default());
        for i in 0..MAX_SESSIONS {
            open_key(&registry, &format!("cap-{i}")).expect("spawn");
        }
        assert_eq!(open_key(&registry, "cap-extra").err(), Some("already running 4 terminals".to_string()));
        assert!(open_key(&registry, "cap-0").is_ok(), "attaching makes nothing new");
        registry.kill_key("cap-1").unwrap();
        assert!(open_key(&registry, "cap-extra").is_ok(), "killing one makes room");
        registry.kill_all();
    }

    #[test]
    fn attach_reports_the_last_detected_agent() {
        let registry = Arc::new(PtyRegistry::default());
        let id = open_key(&registry, "agented").expect("spawn").id;
        registry.note_agent(id, Some("claude".into()));
        assert_eq!(open_key(&registry, "agented").unwrap().agent.as_deref(), Some("claude"));
        registry.note_agent(id, None);
        assert_eq!(open_key(&registry, "agented").unwrap().agent, None);
        registry.kill_all();
    }

    #[test]
    fn detach_all_keeps_the_shell_and_a_killed_key_is_gone() {
        let registry = Arc::new(PtyRegistry::default());
        let id = open_key(&registry, "kept").expect("spawn").id;
        registry.detach_all();
        assert!(registry.contains(id), "still running");
        assert_eq!(registry.list().len(), 1);
        registry.kill_key("kept").unwrap();
        assert!(registry.list().is_empty());
        assert_eq!(registry.kill_key("kept"), Ok(()), "idempotent");
        registry.forget_key("kept");
    }

    #[test]
    fn a_keyed_tombstone_stays_for_every_reload_until_its_pane_closes() {
        let registry = Arc::new(PtyRegistry::default());
        let first = open_key(&registry, "dead").expect("spawn");
        // Nobody here answers ConPTY's opening query, which is what this client
        // leaving has to take care of, or the shell below never exits.
        registry.detach_all();
        thread::sleep(Duration::from_millis(1500));
        let cmd: &[u8] = if cfg!(windows) { b"exit 3\r\n" } else { b"exit 3\n" };
        registry.write(first.id, cmd).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while registry.list().first().is_some_and(|s| !s.exited) {
            assert!(Instant::now() < deadline, "no exit");
            thread::sleep(Duration::from_millis(20));
        }
        // Long enough for EOF: the take after it is what used to drop the session.
        let drain = |client| {
            let end = Instant::now() + Duration::from_secs(2);
            let mut out = Vec::new();
            while Instant::now() < end {
                out.extend(registry.take_from(first.id, client));
                thread::sleep(Duration::from_millis(20));
            }
            out
        };

        for _ in 0..3 {
            let again = open_key(&registry, "dead").expect("attach");
            assert_eq!(again.id, first.id, "the tombstone, not a new shell");
            assert_eq!(again.exit, Some(Some(3)));
            let screen = drain(again.client);
            assert!(String::from_utf8_lossy(&screen).contains("exit 3"), "the final screen");
            registry.detach_all();
        }

        registry.kill_key("dead").unwrap();
        assert!(registry.list().is_empty(), "closing the pane drops it");
    }

    #[test]
    fn unknown_id_errors() {
        let registry = Arc::new(PtyRegistry::default());
        assert_eq!(registry.write(7, b"x"), Err("no pty 7".into()));
        assert_eq!(registry.resize(7, 80, 24), Err("no pty 7".into()));
        assert_eq!(registry.kill(7), Ok(()));
    }
}
