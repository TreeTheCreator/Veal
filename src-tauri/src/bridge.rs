//! The app's end of the daemon (plan-19 section 7). A WebView cannot open a
//! pipe, so this process holds one connection for every pane and keeps the
//! `pty_*` commands' shapes: JS still drains a per-pane outbox with
//! `pty_read` after a `pty-data` signal, as it did when the shells were
//! in-process.
//!
//! The pipe's reader must never wait on one pane, or every pane on the pipe
//! would stall behind it. So output only ever lands in an outbox here; a pane
//! that falls more than `OUTBOX_CAP` behind is cleared and asks the daemon for
//! a snapshot instead.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use veal_core::client::{self, Client, Sink, StartError};
use veal_core::protocol::{ClientMsg, DaemonMsg, ReplyBody, SessionInfo};
use veal_core::session::OUTBOX_CAP;
use veal_core::vlog;

#[derive(Clone, Serialize)]
struct PtyExit {
    id: u32,
    code: Option<i32>,
}

#[derive(Clone, Serialize)]
struct PtyData {
    id: u32,
}

#[derive(Clone, Serialize)]
struct PtyAgent {
    id: u32,
    agent: Option<String>,
}

/// What a pane gets back from `pty_open`.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenedPane {
    pub id: u32,
    pub client: u32,
    pub exited: bool,
    pub code: Option<i32>,
    pub agent: Option<String>,
}

struct Pane {
    key: String,
    /// The daemon's number for the session, on output frames.
    wire: u32,
    /// The daemon's number for this pane's attach, for `detach`.
    client: u32,
    outbox: Vec<u8>,
    /// A `pty-data` is outstanding; JS will drain, so no second one.
    notified: bool,
    /// Bytes were dropped: everything until the snapshot is dropped too.
    resyncing: bool,
}

/// Every pane this window has open, by the ids JS knows them by. App ids come
/// from a counter that never resets, so a restarted daemon's session 1 can never
/// reach a JS map that still holds the old 1 (plan-19 D10).
#[derive(Default)]
pub struct Panes {
    by_app: HashMap<u32, Pane>,
    /// Each session's newest pane: the daemon only sends to the newest attach.
    by_wire: HashMap<u32, u32>,
    by_key: HashMap<String, u32>,
    /// Output for a session whose `pty_open` reply has not been handled yet.
    unclaimed: HashMap<u32, Vec<u8>>,
    /// Exits for a key no pane has claimed yet: a shell that dies at once.
    early_exits: HashMap<String, Option<i32>>,
}

/// What the caller of a `Panes` method has to do outside the lock.
#[derive(Debug, Default, PartialEq)]
pub struct Effects {
    /// Emit `pty-data` for this app id.
    pub notify: Option<u32>,
    /// Send `resync` for this key.
    pub resync: Option<String>,
}

impl Panes {
    pub fn output(&mut self, wire: u32, bytes: &[u8], snapshot: bool) -> Effects {
        let Some(&app) = self.by_wire.get(&wire) else {
            let held = self.unclaimed.entry(wire).or_default();
            if snapshot || held.len() + bytes.len() > OUTBOX_CAP {
                held.clear();
            }
            held.extend_from_slice(bytes);
            return Effects::default();
        };
        let pane = self.by_app.get_mut(&app).expect("by_wire points at a pane");
        let mut fx = Effects::default();
        if snapshot {
            // Starts with a reset, so whatever JS has not drained is moot.
            pane.resyncing = false;
            pane.outbox.clear();
        } else if pane.resyncing {
            return fx;
        } else if pane.outbox.len() + bytes.len() > OUTBOX_CAP {
            pane.outbox.clear();
            pane.resyncing = true;
            fx.resync = Some(pane.key.clone());
            return fx;
        }
        pane.outbox.extend_from_slice(bytes);
        if !pane.notified {
            pane.notified = true;
            fx.notify = Some(app);
        }
        fx
    }

    /// A `pty_open` reply: the pane exists now. Returns an exit it missed.
    /// It becomes the session's current pane unless a newer attach already is,
    /// the same rule the daemon uses, so both agree on who gets output.
    pub fn claim(&mut self, app: u32, key: &str, wire: u32, client: u32) -> Option<Option<i32>> {
        let newest = self.by_wire.get(&wire).and_then(|a| self.by_app.get(a)).is_none_or(|p| p.client < client);
        let outbox = if newest { self.unclaimed.remove(&wire).unwrap_or_default() } else { Vec::new() };
        self.by_app.insert(
            app,
            Pane { key: key.to_string(), wire, client, outbox, notified: false, resyncing: false },
        );
        if newest {
            self.by_wire.insert(wire, app);
            self.by_key.insert(key.to_string(), app);
        }
        self.early_exits.remove(key)
    }

    /// Everything since the last take. The next output signals again.
    pub fn take(&mut self, app: u32) -> Vec<u8> {
        match self.by_app.get_mut(&app) {
            Some(p) => {
                p.notified = false;
                std::mem::take(&mut p.outbox)
            }
            None => Vec::new(),
        }
    }

    /// The pane's terminal went away. Returns what `detach` needs.
    /// If it was current, the newest pane left for the session takes over.
    pub fn remove(&mut self, app: u32) -> Option<(String, u32)> {
        let pane = self.by_app.remove(&app)?;
        if self.by_wire.get(&pane.wire) == Some(&app) {
            self.by_wire.remove(&pane.wire);
            self.by_key.remove(&pane.key);
            let next = self.by_app.iter().filter(|(_, p)| p.wire == pane.wire).max_by_key(|(_, p)| p.client);
            if let Some((&next, p)) = next {
                self.by_wire.insert(pane.wire, next);
                self.by_key.insert(p.key.clone(), next);
            }
        }
        Some((pane.key, pane.client))
    }

    pub fn wire_of(&self, app: u32) -> Option<(u32, String)> {
        self.by_app.get(&app).map(|p| (p.wire, p.key.clone()))
    }

    /// The pane showing `key`, or a note to deliver its exit once one is.
    pub fn exited(&mut self, key: &str, code: Option<i32>) -> Option<u32> {
        let app = self.by_key.get(key).copied();
        if app.is_none() {
            self.early_exits.insert(key.to_string(), code);
        }
        app
    }

    pub fn app_of_key(&self, key: &str) -> Option<u32> {
        self.by_key.get(key).copied()
    }

    /// Every pane's detach, and nothing left.
    pub fn drain(&mut self) -> Vec<(String, u32)> {
        let all = self.by_app.drain().map(|(_, p)| (p.key, p.client)).collect();
        *self = Panes::default();
        all
    }
}

struct Shared {
    app: AppHandle,
    panes: Mutex<Panes>,
    client: Mutex<Option<Arc<Client>>>,
    /// Which connection is current; a stale one's events are ignored.
    generation: AtomicU64,
}

/// The current connection's events, routed into `Panes`.
struct BridgeSink {
    shared: Arc<Shared>,
    generation: u64,
}

impl BridgeSink {
    fn current(&self) -> bool {
        self.shared.generation.load(Ordering::SeqCst) == self.generation
    }

    fn apply(&self, fx: Effects) {
        if let Some(id) = fx.notify {
            let _ = self.shared.app.emit("pty-data", PtyData { id });
        }
        if let Some(key) = fx.resync {
            if let Some(c) = self.shared.client.lock().unwrap().as_ref() {
                c.send(ClientMsg::Resync { key });
            }
        }
    }
}

impl Sink for BridgeSink {
    fn output(&self, session: u32, bytes: Vec<u8>) {
        if self.current() {
            let fx = self.shared.panes.lock().unwrap().output(session, &bytes, false);
            self.apply(fx);
        }
    }

    fn snapshot(&self, session: u32, bytes: Vec<u8>) {
        if self.current() {
            let fx = self.shared.panes.lock().unwrap().output(session, &bytes, true);
            self.apply(fx);
        }
    }

    fn event(&self, msg: DaemonMsg) {
        if !self.current() {
            return;
        }
        match msg {
            DaemonMsg::Exit { key, code } => {
                if let Some(id) = self.shared.panes.lock().unwrap().exited(&key, code) {
                    let _ = self.shared.app.emit("pty-exit", PtyExit { id, code });
                }
            }
            DaemonMsg::Agent { key, agent } => {
                if let Some(id) = self.shared.panes.lock().unwrap().app_of_key(&key) {
                    let _ = self.shared.app.emit("pty-agent", PtyAgent { id, agent });
                }
            }
            // Plan-20's phone uses these; this window learns of sessions at boot.
            DaemonMsg::SessionAdded { .. } | DaemonMsg::SessionRemoved { .. } | DaemonMsg::Reply { .. } => {}
        }
    }

    fn disconnected(&self) {
        if !self.current() {
            return;
        }
        vlog!("[bridge] daemon connection lost");
        self.shared.client.lock().unwrap().take();
        self.shared.panes.lock().unwrap().drain();
        let _ = self.shared.app.emit("pty-daemon-lost", ());
    }
}

type Rgb = (u8, u8, u8);

pub struct Bridge {
    shared: Arc<Shared>,
    /// One connect at a time; commands wait on the same one.
    connecting: tauri::async_runtime::Mutex<()>,
    next_app: AtomicU32,
    theme: Mutex<Option<(Rgb, Rgb)>>,
    /// The close guard chose Keep running: closing detaches, not `shutdown`
    /// (plan-19 E3).
    keep_on_close: AtomicBool,
}

impl Bridge {
    pub fn new(app: AppHandle) -> Bridge {
        Bridge {
            shared: Arc::new(Shared {
                app,
                panes: Mutex::new(Panes::default()),
                client: Mutex::new(None),
                generation: AtomicU64::new(0),
            }),
            connecting: tauri::async_runtime::Mutex::new(()),
            next_app: AtomicU32::new(1),
            theme: Mutex::new(None),
            keep_on_close: AtomicBool::new(false),
        }
    }

    fn current(&self) -> Option<Arc<Client>> {
        self.shared.client.lock().unwrap().clone().filter(|c| !c.is_closed())
    }

    /// The connection, made or remade as needed, launching the daemon if
    /// there is none. A daemon that died comes back here: the next `pty_open`
    /// (the pane's Restart) starts a new one.
    pub async fn client(&self) -> Result<Arc<Client>, String> {
        if let Some(c) = self.current() {
            return Ok(c);
        }
        let _one = self.connecting.lock().await;
        if let Some(c) = self.current() {
            return Ok(c);
        }
        let app_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let (exe, build) =
            client::daemon_exe(&app_exe, cfg!(debug_assertions)).map_err(|e| format!("background service: {e}"))?;
        let name = veal_core::pipe::pipe_name().map_err(|e| e.to_string())?;
        let generation = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let sink = Arc::new(BridgeSink { shared: Arc::clone(&self.shared), generation });
        let c = client::connect_or_launch(&name, &exe, &build, cfg!(debug_assertions), sink)
            .await
            .map_err(|e: StartError| e.to_string())?;
        let c = Arc::new(c);
        vlog!("[bridge] connected to daemon build {} ({} sessions)", c.daemon.build, c.daemon.sessions);
        if let Some((fg, bg)) = *self.theme.lock().unwrap() {
            c.send(ClientMsg::Theme { fg, bg });
        }
        *self.shared.client.lock().unwrap() = Some(Arc::clone(&c));
        Ok(c)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn open(
        &self,
        key: String,
        cwd: String,
        cols: u16,
        rows: u16,
        shell: Option<String>,
        proxy_mode: Option<String>,
        proxy_port: Option<String>,
        proxy_bypass: Option<String>,
        env: Option<Vec<(String, String)>>,
    ) -> Result<OpenedPane, String> {
        let c = self.client().await?;
        let msg = ClientMsg::Spawn {
            key: key.clone(),
            cwd,
            cols,
            rows,
            shell,
            proxy_mode,
            proxy_port,
            proxy_bypass,
            env,
            responder: true,
            controller: true,
        };
        let ReplyBody::Opened { session, client, exited, code, agent } = c.request(msg).await? else {
            return Err("unexpected reply to spawn".into());
        };
        let id = self.next_app.fetch_add(1, Ordering::Relaxed);
        let missed = self.shared.panes.lock().unwrap().claim(id, &key, session, client);
        if let Some(code) = missed {
            // Before the reply reaches JS, whose exit listener holds it as early.
            let _ = self.shared.app.emit("pty-exit", PtyExit { id, code });
        }
        Ok(OpenedPane { id, client, exited, code, agent })
    }

    pub async fn list(&self) -> Result<Vec<SessionInfo>, String> {
        match self.client().await?.request(ClientMsg::List).await? {
            ReplyBody::List { sessions } => Ok(sessions),
            other => Err(format!("unexpected reply to list: {other:?}")),
        }
    }

    pub fn read(&self, id: u32) -> Vec<u8> {
        self.shared.panes.lock().unwrap().take(id)
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let wire = self.shared.panes.lock().unwrap().wire_of(id).map(|(w, _)| w);
        match (wire, self.current()) {
            (Some(w), Some(c)) => {
                c.input(w, data.to_vec());
                Ok(())
            }
            _ => Err(format!("no pty {id}")),
        }
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let key = self.shared.panes.lock().unwrap().wire_of(id).map(|(_, k)| k);
        match (key, self.current()) {
            (Some(key), Some(c)) => {
                c.send(ClientMsg::Resize { key, cols, rows });
                Ok(())
            }
            _ => Err(format!("no pty {id}")),
        }
    }

    pub fn detach(&self, id: u32) {
        let gone = self.shared.panes.lock().unwrap().remove(id);
        if let (Some((key, client)), Some(c)) = (gone, self.current()) {
            c.send(ClientMsg::Detach { key, client });
        }
    }

    pub async fn kill(&self, key: String) -> Result<(), String> {
        self.client().await?.request(ClientMsg::Kill { key }).await.map(drop)
    }

    pub async fn forget(&self, key: String) -> Result<(), String> {
        self.client().await?.request(ClientMsg::Forget { key }).await.map(drop)
    }

    pub fn theme(&self, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        *self.theme.lock().unwrap() = Some((fg, bg));
        if let Some(c) = self.current() {
            c.send(ClientMsg::Theme { fg, bg });
        }
    }

    /// A fresh page never ran the old page's cleanup: detach every pane it had.
    /// Sent on the same pipe as the new page's opens, so ahead of them.
    pub fn detach_all(&self) {
        let all = self.shared.panes.lock().unwrap().drain();
        if let Some(c) = self.current() {
            for (key, client) in all {
                c.send(ClientMsg::Detach { key, client });
            }
        }
    }

    /// Set by the close guard just before the window goes.
    pub fn set_keep_on_close(&self, keep: bool) {
        self.keep_on_close.store(keep, Ordering::SeqCst);
    }

    /// The window is going for good. Keep running leaves every shell to the
    /// daemon and its tray; anything else ends them, so idle shells do not
    /// pile up unseen (plan-19 section 8).
    pub async fn close(&self) {
        if self.keep_on_close.load(Ordering::SeqCst) {
            vlog!("[bridge] closing; shells keep running");
            self.detach_all();
            return;
        }
        self.shutdown().await;
    }

    /// End every shell and the daemon.
    pub async fn shutdown(&self) {
        if let Some(c) = self.current() {
            c.shutdown(Duration::from_secs(2)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_signals_once_until_taken() {
        let mut p = Panes::default();
        p.claim(1, "k", 10, 0);
        assert_eq!(p.output(10, b"a", false).notify, Some(1));
        assert_eq!(p.output(10, b"b", false).notify, None);
        assert_eq!(p.take(1), b"ab");
        assert_eq!(p.output(10, b"c", false).notify, Some(1));
    }

    #[test]
    fn overflow_asks_once_and_drops_until_the_snapshot() {
        let mut p = Panes::default();
        p.claim(1, "k", 10, 0);
        p.output(10, &vec![b'x'; OUTBOX_CAP - 1], false);
        let fx = p.output(10, b"yy", false);
        assert_eq!(fx.resync.as_deref(), Some("k"));
        assert_eq!(p.output(10, b"late", false), Effects::default(), "dropped, no second resync");
        assert!(p.take(1).is_empty(), "cleared");
        let fx = p.output(10, b"\x1bcscreen", true);
        assert_eq!(fx.notify, Some(1));
        assert_eq!(p.take(1), b"\x1bcscreen");
        p.output(10, b"more", false);
        assert_eq!(p.take(1), b"more", "back to the stream");
    }

    #[test]
    fn output_before_the_reply_is_kept_for_the_pane() {
        let mut p = Panes::default();
        p.output(10, b"\x1b[6n", false);
        p.claim(1, "k", 10, 0);
        assert_eq!(p.take(1), b"\x1b[6n");
    }

    #[test]
    fn an_exit_before_the_reply_is_handed_to_the_pane() {
        let mut p = Panes::default();
        assert_eq!(p.exited("k", Some(3)), None);
        assert_eq!(p.claim(1, "k", 10, 0), Some(Some(3)));
        assert_eq!(p.exited("k", Some(0)), Some(1));
    }

    #[test]
    fn the_newest_pane_for_a_session_gets_its_output() {
        // React's double effect: open, open, then the first detaches.
        let mut p = Panes::default();
        p.claim(1, "k", 10, 0);
        p.claim(2, "k", 10, 1);
        assert_eq!(p.remove(1), Some(("k".into(), 0)));
        assert_eq!(p.output(10, b"x", false).notify, Some(2));
        assert_eq!(p.app_of_key("k"), Some(2));
    }

    #[test]
    fn the_older_pane_takes_over_when_the_newer_goes() {
        // What React's double effect did live: the newer attach is the one
        // that detaches, and the older pane stays.
        let mut p = Panes::default();
        p.claim(1, "k", 10, 0);
        p.claim(2, "k", 10, 1);
        assert_eq!(p.remove(2), Some(("k".into(), 1)));
        assert_eq!(p.output(10, b"x", false).notify, Some(1));
        assert_eq!(p.app_of_key("k"), Some(1));
        assert_eq!(p.take(1), b"x");
    }

    #[test]
    fn a_late_claim_of_an_older_attach_does_not_take_over() {
        let mut p = Panes::default();
        p.claim(2, "k", 10, 1);
        p.claim(1, "k", 10, 0);
        assert_eq!(p.output(10, b"x", false).notify, Some(2));
    }

    #[test]
    fn drain_empties_everything() {
        let mut p = Panes::default();
        p.claim(1, "a", 10, 0);
        p.claim(2, "b", 11, 0);
        let mut all = p.drain();
        all.sort();
        assert_eq!(all, [("a".into(), 0), ("b".into(), 0)]);
        assert!(p.take(1).is_empty());
        assert_eq!(p.output(10, b"x", false), Effects::default(), "a stale wire id goes nowhere");
    }
}
