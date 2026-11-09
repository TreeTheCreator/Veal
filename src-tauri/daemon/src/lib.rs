//! The daemon (plan-19 section 6): a listener in front of `PtyRegistry`. Every
//! shell lives here, so closing or reloading the window, or the UI crashing,
//! does not end an agent. One task per connection reads requests, and one
//! task per connection owns the pipe's write half; the PTY threads never
//! touch the pipe.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use veal_core::protocol::{self, ClientMsg, DaemonMsg, Decoder, Frame, ReplyBody, Request, PROTOCOL};
use veal_core::pty::{DataReady, PtyRegistry, SpawnOptions};
use veal_core::session::ClientId;
use veal_core::{agents, proxy, vlog};

/// How long the daemon waits with no sessions and no connections.
pub const IDLE: Duration = Duration::from_secs(30);

/// What a connection's writer is asked to do, in order.
enum Out {
    Frame(Vec<u8>),
    /// A session has bytes for this connection's current client of it.
    Ready(u32),
    /// An attach or spawn finished: record it, then reply. Done by the writer,
    /// so no output for the session can reach the pipe before its reply does.
    Opened { key: String, id: u32, client: ClientId, reply: Vec<u8> },
    /// The reply to `shutdown` is on the pipe; now stop everything.
    Shutdown,
}

/// The clients one connection holds. A page can hold two attaches to one
/// session (React's double effect), and either may be the one that stays, so
/// "current" is always the newest one still attached.
#[derive(Default)]
struct Held {
    /// Each key's current client, for `resize`, `resync` and `detach`.
    by_key: HashMap<String, (u32, ClientId)>,
    /// Each session's current client, for `Ready`.
    by_id: HashMap<u32, ClientId>,
    /// Every client still attached, for cleanup when the pipe closes.
    all: HashSet<(u32, ClientId)>,
}

impl Held {
    /// Record an attach. It becomes current unless a newer one already is.
    fn add(&mut self, key: String, sid: u32, client: ClientId) {
        self.all.insert((sid, client));
        if self.by_id.get(&sid).is_none_or(|&c| c < client) {
            self.by_id.insert(sid, client);
            self.by_key.insert(key, (sid, client));
        }
    }

    /// Drop an attach. If it was current, the newest one left takes over;
    /// returns the session if that happened, so its queue gets taken.
    fn remove(&mut self, key: &str, sid: u32, client: ClientId) -> Option<u32> {
        self.all.remove(&(sid, client));
        if self.by_id.get(&sid) != Some(&client) {
            return None;
        }
        self.by_id.remove(&sid);
        self.by_key.remove(key);
        let next = self.all.iter().filter(|(s, _)| *s == sid).map(|(_, c)| *c).max()?;
        self.by_id.insert(sid, next);
        self.by_key.insert(key.to_string(), (sid, next));
        Some(sid)
    }
}

pub struct State {
    pub registry: Arc<PtyRegistry>,
    conns: Mutex<HashMap<u64, mpsc::UnboundedSender<Out>>>,
    next_conn: AtomicU64,
    build: String,
    /// When the daemon last had a connection or a live session.
    busy_at: Mutex<Instant>,
    /// Exits the process; a test swaps it out.
    exit: Box<dyn Fn() + Send + Sync>,
}

impl State {
    pub fn new(build: String) -> Arc<State> {
        Self::with_exit(build, Box::new(|| std::process::exit(0)))
    }

    pub fn with_exit(build: String, exit: Box<dyn Fn() + Send + Sync>) -> Arc<State> {
        Arc::new(State {
            registry: Arc::new(PtyRegistry::default()),
            conns: Mutex::new(HashMap::new()),
            next_conn: AtomicU64::new(1),
            build,
            busy_at: Mutex::new(Instant::now()),
            exit,
        })
    }

    fn broadcast(&self, msg: &DaemonMsg) {
        let frame = Frame::control(msg).encode();
        for tx in self.conns.lock().unwrap().values() {
            let _ = tx.send(Out::Frame(frame.clone()));
        }
    }

    /// Starts agent detection. A change goes to every connection.
    pub fn start_agents(self: &Arc<Self>) {
        let state = Arc::downgrade(self);
        agents::start(Arc::clone(&self.registry), move |a| {
            let Some(state) = state.upgrade() else { return };
            if let Some(key) = state.registry.key_of(a.id) {
                state.broadcast(&DaemonMsg::Agent { key, agent: a.agent });
            }
        });
    }

    /// End every shell, then the process. Waits a little for the shells to go,
    /// so none outlives the daemon by much.
    pub fn shutdown(&self) {
        vlog!("[daemon] shutdown");
        self.registry.kill_all();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.registry.pids().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        (self.exit)();
    }

    /// Whether the daemon has had nothing to do for `idle`.
    pub fn idle_for(&self, idle: Duration) -> bool {
        let mut busy_at = self.busy_at.lock().unwrap();
        if !self.conns.lock().unwrap().is_empty() || self.registry.live_count() > 0 {
            *busy_at = Instant::now();
            return false;
        }
        busy_at.elapsed() >= idle
    }
}

fn reply(id: u64, result: Result<ReplyBody, String>) -> Vec<u8> {
    Frame::control(&DaemonMsg::Reply { id, result }).encode()
}

/// Serve one connection until it closes or sends garbage.
pub async fn serve<S>(state: Arc<State>, stream: S)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let conn = state.next_conn.fetch_add(1, Ordering::Relaxed);
    let (mut rd, wr) = tokio::io::split(stream);
    let (tx, rx) = mpsc::unbounded_channel::<Out>();
    let held = Arc::new(Mutex::new(Held::default()));
    let writer = tokio::spawn(write_loop(Arc::clone(&state), wr, rx, Arc::clone(&held)));
    vlog!("[daemon] conn {conn} open");

    let mut decoder = Decoder::default();
    let mut buf = vec![0u8; 64 * 1024];
    let mut hello = false;
    let mut matched = false;
    'read: loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        decoder.push(&buf[..n]);
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(f)) => f,
                Ok(None) => break,
                Err(e) => {
                    vlog!("[daemon] conn {conn}: {e}");
                    break 'read;
                }
            };
            match frame {
                Frame::Control(json) => {
                    let req = match protocol::parse::<Request>(&json) {
                        Ok(r) => r,
                        Err(e) => {
                            vlog!("[daemon] conn {conn}: {e}");
                            break 'read;
                        }
                    };
                    if !hello {
                        let ClientMsg::Hello { protocol: theirs, build, role } = &req.msg else {
                            vlog!("[daemon] conn {conn}: first message was not hello");
                            break 'read;
                        };
                        hello = true;
                        matched = *theirs == PROTOCOL;
                        vlog!("[daemon] conn {conn}: hello {role} protocol {theirs} build {build}");
                        let body = ReplyBody::Hello {
                            protocol: PROTOCOL,
                            build: state.build.clone(),
                            sessions: state.registry.live_count(),
                        };
                        let _ = tx.send(Out::Frame(reply(req.id, Ok(body))));
                        if matched {
                            state.conns.lock().unwrap().insert(conn, tx.clone());
                        }
                        continue;
                    }
                    // A client of another protocol may only look, or end it all.
                    if !matched && !matches!(req.msg, ClientMsg::List | ClientMsg::Shutdown) {
                        let _ = tx.send(Out::Frame(reply(req.id, Err("protocol mismatch".into()))));
                        continue;
                    }
                    dispatch(&state, &tx, &held, req);
                }
                Frame::Input { session, bytes } => {
                    let attached = held.lock().unwrap().by_id.contains_key(&session);
                    if attached {
                        if let Err(e) = state.registry.write(session, &bytes) {
                            vlog!("[daemon] conn {conn}: write: {e}");
                        }
                    }
                }
                Frame::Output { .. } | Frame::Snapshot { .. } => {
                    vlog!("[daemon] conn {conn}: client sent output");
                    break 'read;
                }
            }
        }
    }

    // The pipe is gone or not to be trusted: every client it held goes, and
    // every shell stays.
    state.conns.lock().unwrap().remove(&conn);
    let all: Vec<_> = held.lock().unwrap().all.drain().collect();
    for (id, client) in all {
        state.registry.detach(id, client);
    }
    drop(tx);
    let _ = writer.await;
    vlog!("[daemon] conn {conn} closed");
}

fn dispatch(state: &Arc<State>, tx: &mpsc::UnboundedSender<Out>, held: &Arc<Mutex<Held>>, req: Request) {
    let id = req.id;
    let answer = |result: Result<ReplyBody, String>| {
        if id != 0 {
            let _ = tx.send(Out::Frame(reply(id, result)));
        }
    };
    let ready = |tx: &mpsc::UnboundedSender<Out>| -> DataReady {
        let tx = tx.clone();
        Arc::new(move |sid: u32| {
            let _ = tx.send(Out::Ready(sid));
        })
    };
    match req.msg {
        ClientMsg::Hello { .. } => answer(Err("hello twice".into())),
        ClientMsg::List => answer(Ok(ReplyBody::List { sessions: state.registry.list() })),
        ClientMsg::Spawn { key, cwd, cols, rows, shell, proxy_mode, proxy_port, proxy_bypass, env, .. } => {
            // Resolving the proxy can block, and so can starting a process: off
            // the read loop, so other panes keep flowing (plan-19 D1).
            let state = Arc::clone(state);
            let tx = tx.clone();
            let ready = ready(&tx);
            tokio::task::spawn_blocking(move || {
                let before = state.registry.id_for(&key);
                let weak = Arc::downgrade(&state);
                let exit_key = key.clone();
                let on_exit = move |_id: u32, code: Option<i32>| {
                    if let Some(state) = weak.upgrade() {
                        state.broadcast(&DaemonMsg::Exit { key: exit_key, code });
                    }
                };
                let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let resolved = proxy::resolve_setting(proxy_mode.as_deref(), proxy_port.as_deref());
                    let penv = proxy::proxy_env(&resolved, proxy_bypass.as_deref());
                    let opts = SpawnOptions { cwd, cols, rows, shell, env_overlay: env };
                    state.registry.open(&key, opts, penv, ready, on_exit)
                }))
                .unwrap_or_else(|_| Err("spawn panicked".into()));
                match opened {
                    Ok(o) => {
                        if before.is_none() {
                            state.broadcast(&DaemonMsg::SessionAdded { key: key.clone() });
                        }
                        let body = ReplyBody::Opened {
                            session: o.id,
                            client: o.client,
                            exited: o.exit.is_some(),
                            code: o.exit.flatten(),
                            agent: o.agent,
                        };
                        let _ = tx.send(Out::Opened { key, id: o.id, client: o.client, reply: reply(id, Ok(body)) });
                    }
                    Err(e) => {
                        let _ = tx.send(Out::Frame(reply(id, Err(e))));
                    }
                }
            });
        }
        ClientMsg::Attach { key, cols, rows, responder, controller } => {
            match state.registry.attach_key_as(&key, cols, rows, responder, controller, ready(tx)) {
                Some(o) => {
                    let body = ReplyBody::Opened {
                        session: o.id,
                        client: o.client,
                        exited: o.exit.is_some(),
                        code: o.exit.flatten(),
                        agent: o.agent,
                    };
                    let _ = tx.send(Out::Opened { key, id: o.id, client: o.client, reply: reply(id, Ok(body)) });
                }
                None => answer(Err(format!("no session {key}"))),
            }
        }
        ClientMsg::Detach { key, client } => {
            let mut h = held.lock().unwrap();
            let sid = state.registry.id_for(&key).or_else(|| h.by_key.get(&key).map(|(s, _)| *s));
            if let Some(sid) = sid.filter(|s| h.all.contains(&(*s, client))) {
                let promoted = h.remove(&key, sid, client);
                drop(h);
                state.registry.detach(sid, client);
                if let Some(sid) = promoted {
                    let _ = tx.send(Out::Ready(sid));
                }
            }
            answer(Ok(ReplyBody::Ok));
        }
        ClientMsg::Resize { key, cols, rows } => {
            let sid = held.lock().unwrap().by_key.get(&key).map(|(sid, _)| *sid);
            let result = match sid {
                Some(sid) => state.registry.resize(sid, cols, rows).map(|()| ReplyBody::Ok),
                None => Err(format!("not attached to {key}")),
            };
            answer(result);
        }
        ClientMsg::Resync { key } => {
            if let Some((sid, client)) = held.lock().unwrap().by_key.get(&key).copied() {
                state.registry.resync(sid, client);
            }
            answer(Ok(ReplyBody::Ok));
        }
        ClientMsg::Kill { key } => {
            let existed = state.registry.id_for(&key).is_some();
            let result = state.registry.kill_key(&key).map(|()| ReplyBody::Ok);
            {
                let mut h = held.lock().unwrap();
                if let Some((sid, c)) = h.by_key.remove(&key) {
                    h.by_id.remove(&sid);
                    h.all.remove(&(sid, c));
                }
            }
            if existed {
                state.broadcast(&DaemonMsg::SessionRemoved { key });
            }
            answer(result);
        }
        ClientMsg::Forget { key } => {
            let existed = state.registry.id_for(&key).is_some();
            state.registry.forget_key(&key);
            if existed && state.registry.id_for(&key).is_none() {
                state.broadcast(&DaemonMsg::SessionRemoved { key });
            }
            answer(Ok(ReplyBody::Ok));
        }
        ClientMsg::Theme { fg, bg } => {
            state.registry.set_theme_all(fg, bg);
            answer(Ok(ReplyBody::Ok));
        }
        ClientMsg::Shutdown => {
            answer(Ok(ReplyBody::Ok));
            let _ = tx.send(Out::Shutdown);
        }
    }
}

async fn write_loop<W>(state: Arc<State>, mut wr: W, mut rx: mpsc::UnboundedReceiver<Out>, held: Arc<Mutex<Held>>)
where
    W: AsyncWrite + Unpin,
{
    while let Some(out) = rx.recv().await {
        let bytes = match out {
            Out::Frame(b) => b,
            Out::Ready(sid) => {
                let Some(client) = held.lock().unwrap().by_id.get(&sid).copied() else { continue };
                match take(&state, sid, client) {
                    Some(b) => b,
                    None => continue,
                }
            }
            Out::Opened { key, id: sid, client, reply } => {
                held.lock().unwrap().add(key, sid, client);
                if wr.write_all(&reply).await.is_err() {
                    break;
                }
                // A signal that came before the record was dropped: take now.
                match take(&state, sid, client) {
                    Some(b) => b,
                    None => continue,
                }
            }
            Out::Shutdown => {
                let _ = wr.flush().await;
                let state = Arc::clone(&state);
                tokio::task::spawn_blocking(move || state.shutdown());
                continue;
            }
        };
        if wr.write_all(&bytes).await.is_err() {
            break;
        }
    }
}

/// One take as a frame, or nothing if there was nothing.
fn take(state: &State, sid: u32, client: ClientId) -> Option<Vec<u8>> {
    let taken = state.registry.take_taken(sid, client);
    if taken.bytes.is_empty() && !taken.snapshot {
        return None;
    }
    let frame = if taken.snapshot {
        Frame::Snapshot { session: sid, bytes: taken.bytes }
    } else {
        Frame::Output { session: sid, bytes: taken.bytes }
    };
    Some(frame.encode())
}

/// Accept connections on `name` until the process exits. Returns `Ok` at once
/// if another daemon already serves the name.
#[cfg(windows)]
pub async fn run(state: Arc<State>, name: &str, idle: Duration) -> std::io::Result<()> {
    use veal_core::pipe;
    let mut server = match pipe::server(name, true) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            vlog!("[daemon] another daemon has {name}; exiting");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    vlog!("[daemon] listening on {name}, build {}", state.build);
    state.start_agents();

    let idle_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(250).min(idle));
        loop {
            tick.tick().await;
            if idle_state.idle_for(idle) {
                vlog!("[daemon] idle for {idle:?}; exiting");
                (idle_state.exit)();
            }
        }
    });

    loop {
        server.connect().await?;
        // The next instance exists before this one is handed off, so a client
        // never finds the name with no instance to open.
        let next = pipe::server(name, false)?;
        let conn = std::mem::replace(&mut server, next);
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let task = tokio::spawn(serve(state, conn));
            if let Err(e) = task.await {
                if e.is_panic() {
                    vlog!("[daemon] a connection panicked: {e}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::DuplexStream;

    struct Peer {
        io: DuplexStream,
        decoder: Decoder,
        next: u64,
    }

    impl Peer {
        async fn send(&mut self, msg: ClientMsg) -> u64 {
            self.next += 1;
            let frame = Frame::control(&Request { id: self.next, msg }).encode();
            self.io.write_all(&frame).await.unwrap();
            self.next
        }

        async fn frame(&mut self) -> Option<Frame> {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                if let Some(f) = self.decoder.next_frame().unwrap() {
                    return Some(f);
                }
                let n = tokio::time::timeout(Duration::from_secs(10), self.io.read(&mut buf)).await.ok()?.ok()?;
                if n == 0 {
                    return None;
                }
                self.decoder.push(&buf[..n]);
            }
        }

        async fn req(&mut self, msg: ClientMsg) -> Result<ReplyBody, String> {
            let id = self.send(msg).await;
            self.reply(id).await
        }

        /// The reply to `id`, skipping output and events.
        async fn reply(&mut self, id: u64) -> Result<ReplyBody, String> {
            loop {
                match self.frame().await.expect("pipe closed") {
                    Frame::Control(json) => {
                        if let DaemonMsg::Reply { id: got, result } = protocol::parse(&json).unwrap() {
                            if got == id {
                                return result;
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }

        /// Output until `needle` shows up.
        async fn until(&mut self, needle: &str) -> (String, bool) {
            let mut seen = Vec::new();
            let mut snapshot = false;
            loop {
                match self.frame().await {
                    Some(Frame::Output { bytes, .. }) => seen.extend(bytes),
                    Some(Frame::Snapshot { bytes, .. }) => {
                        snapshot = true;
                        seen.extend(bytes)
                    }
                    Some(_) => {}
                    None => panic!("no {needle:?} in {:?}", String::from_utf8_lossy(&seen)),
                }
                if String::from_utf8_lossy(&seen).contains(needle) {
                    return (String::from_utf8_lossy(&seen).into_owned(), snapshot);
                }
            }
        }
    }

    fn connect(state: &Arc<State>) -> Peer {
        let (a, b) = tokio::io::duplex(1 << 20);
        tokio::spawn(serve(Arc::clone(state), b));
        Peer { io: a, decoder: Decoder::default(), next: 0 }
    }

    async fn hello(state: &Arc<State>, protocol: u32) -> (Peer, Result<ReplyBody, String>) {
        let mut p = connect(state);
        let id = p.send(ClientMsg::Hello { protocol, build: "t".into(), role: "test".into() }).await;
        let r = p.reply(id).await;
        (p, r)
    }

    fn spawn_msg(key: &str) -> ClientMsg {
        ClientMsg::Spawn {
            key: key.into(),
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cols: 80,
            rows: 24,
            shell: Some(if cfg!(windows) { "cmd.exe".into() } else { "/bin/sh".into() }),
            proxy_mode: Some("off".into()),
            proxy_port: None,
            proxy_bypass: None,
            env: None,
            responder: true,
            controller: true,
        }
    }

    fn test_state() -> Arc<State> {
        State::with_exit("t".into(), Box::new(|| {}))
    }

    /// Answer ConPTY's opening cursor query as xterm would, then wait for a prompt.
    async fn ready_shell(p: &mut Peer, session: u32) {
        if cfg!(windows) {
            p.until("\x1b[6n").await;
            p.io.write_all(&Frame::Input { session, bytes: b"\x1b[1;1R".to_vec() }.encode()).await.unwrap();
        }
        p.until(">").await;
    }

    fn opened(r: Result<ReplyBody, String>) -> (u32, u32) {
        match r {
            Ok(ReplyBody::Opened { session, client, .. }) => (session, client),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_newest_attach_left_is_current() {
        let mut h = Held::default();
        h.add("k".into(), 1, 0);
        h.add("k".into(), 1, 1);
        assert_eq!(h.by_id.get(&1), Some(&1));
        // The newer one goes first: the older takes over.
        assert_eq!(h.remove("k", 1, 1), Some(1));
        assert_eq!(h.by_id.get(&1), Some(&0));
        assert_eq!(h.by_key.get("k"), Some(&(1, 0)));
        // A late record of an older attach does not displace a newer one.
        h.add("k".into(), 1, 2);
        h.add("k".into(), 1, 0);
        assert_eq!(h.by_id.get(&1), Some(&2));
        // A non-current one going changes nothing.
        assert_eq!(h.remove("k", 1, 0), None);
        assert_eq!(h.remove("k", 1, 2), None, "nothing left to take over");
        assert!(h.by_id.is_empty() && h.by_key.is_empty() && h.all.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wrong_protocol_may_only_list_or_shut_down() {
        let state = test_state();
        let (mut p, r) = hello(&state, PROTOCOL + 1).await;
        assert!(matches!(r, Ok(ReplyBody::Hello { protocol: PROTOCOL, .. })), "{r:?}");
        let id = p.send(spawn_msg("k")).await;
        assert_eq!(p.reply(id).await, Err("protocol mismatch".into()));
        let id = p.send(ClientMsg::List).await;
        assert!(matches!(p.reply(id).await, Ok(ReplyBody::List { .. })));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn first_message_must_be_hello() {
        let state = test_state();
        let mut p = connect(&state);
        p.send(ClientMsg::List).await;
        assert!(p.frame().await.is_none(), "closed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn spawn_is_idempotent_and_capped() {
        let state = test_state();
        let (mut p, _) = hello(&state, PROTOCOL).await;
        let a = opened(p.req(spawn_msg("a")).await);
        let again = opened(p.req(spawn_msg("a")).await);
        assert_eq!(a.0, again.0, "same session");
        for k in ["b", "c", "d"] {
            opened(p.req(spawn_msg(k)).await);
        }
        let id = p.send(spawn_msg("e")).await;
        assert!(p.reply(id).await.unwrap_err().contains("already running"));
        state.registry.kill_all();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dropped_connection_detaches_and_the_shell_stays() {
        let state = test_state();
        let (mut p, _) = hello(&state, PROTOCOL).await;
        let (sid, _) = opened(p.req(spawn_msg("keep")).await);
        ready_shell(&mut p, sid).await;
        p.io.write_all(&Frame::Input { session: sid, bytes: b"echo marker-one\r\n".to_vec() }.encode()).await.unwrap();
        p.until("marker-one\r\n").await;
        drop(p);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(state.registry.live_count(), 1);

        let (mut q, _) = hello(&state, PROTOCOL).await;
        let id = q
            .send(ClientMsg::Attach { key: "keep".into(), cols: 80, rows: 24, responder: true, controller: true })
            .await;
        let (again, _) = opened(q.reply(id).await);
        assert_eq!(again, sid);
        let (screen, snapshot) = q.until("marker-one").await;
        assert!(snapshot, "{screen:?}");
        state.registry.kill_all();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resync_sends_a_snapshot_next() {
        let state = test_state();
        let (mut p, _) = hello(&state, PROTOCOL).await;
        let (sid, _) = opened(p.req(spawn_msg("r")).await);
        ready_shell(&mut p, sid).await;
        p.send(ClientMsg::Resync { key: "r".into() }).await;
        loop {
            match p.frame().await.expect("frame") {
                Frame::Snapshot { bytes, .. } => {
                    assert!(bytes.starts_with(b"\x1bc"));
                    break;
                }
                _ => continue,
            }
        }
        state.registry.kill_all();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn detach_names_its_client() {
        // A page that attaches twice to one key and detaches the first must
        // keep the second.
        let state = test_state();
        let (mut p, _) = hello(&state, PROTOCOL).await;
        let (sid, first) = opened(p.req(spawn_msg("twice")).await);
        ready_shell(&mut p, sid).await;
        let id = p
            .send(ClientMsg::Attach { key: "twice".into(), cols: 80, rows: 24, responder: true, controller: true })
            .await;
        let (_, second) = opened(p.reply(id).await);
        assert_ne!(first, second);
        let id = p.send(ClientMsg::Detach { key: "twice".into(), client: first }).await;
        p.reply(id).await.unwrap();
        p.io.write_all(&Frame::Input { session: sid, bytes: b"echo still-here\r\n".to_vec() }.encode()).await.unwrap();
        p.until("still-here\r\n").await;
        state.registry.kill_all();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_kills_everything_and_exits() {
        let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&exited);
        let state = State::with_exit("t".into(), Box::new(move || flag.store(true, Ordering::SeqCst)));
        let (mut p, _) = hello(&state, PROTOCOL).await;
        opened(p.req(spawn_msg("x")).await);
        let id = p.send(ClientMsg::Shutdown).await;
        p.reply(id).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !exited.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "did not exit");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(state.registry.pids().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn idle_only_with_nothing_running_and_nobody_connected() {
        let state = test_state();
        assert!(!state.idle_for(Duration::from_millis(50)));
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(state.idle_for(Duration::from_millis(50)));
        let (_p, _) = hello(&state, PROTOCOL).await;
        assert!(!state.idle_for(Duration::ZERO), "a connection keeps it busy");
    }
}
