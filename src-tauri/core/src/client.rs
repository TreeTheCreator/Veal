//! The app's side of the pipe (plan-19 sections 6 and 7). One reader task
//! routes what the daemon sends: replies to whoever asked, output and events
//! to a `Sink`. The sink runs on that task and must never block, or one slow
//! pane would stall every other pane on the same pipe.

use crate::protocol::{self, ClientMsg, DaemonMsg, Decoder, Frame, ReplyBody, Request, PROTOCOL};
use crate::vlog;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};

pub trait Sink: Send + Sync + 'static {
    fn output(&self, session: u32, bytes: Vec<u8>);
    fn snapshot(&self, session: u32, bytes: Vec<u8>);
    fn event(&self, msg: DaemonMsg);
    /// The pipe closed. Called once, after every pending request has failed.
    fn disconnected(&self);
}

type Pending = HashMap<u64, oneshot::Sender<Result<ReplyBody, String>>>;

/// What the daemon said in reply to `hello`.
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonInfo {
    pub protocol: u32,
    pub build: String,
    /// Live sessions when this client connected.
    pub sessions: usize,
}

pub struct Client {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    /// `None` once the pipe has closed.
    pending: Arc<Mutex<Option<Pending>>>,
    next: AtomicU64,
    closed: watch::Receiver<bool>,
    /// Set before a deliberate shutdown, so its disconnect is not reported.
    quiet: Arc<AtomicBool>,
    /// Dropped with the client, which ends the reader and closes the pipe.
    _close: oneshot::Sender<()>,
    pub daemon: DaemonInfo,
}

pub const DISCONNECTED: &str = "background service disconnected";

impl Client {
    /// Open the daemon's pipe, check who serves it, and say hello.
    #[cfg(windows)]
    pub async fn connect(name: &str, role: &str, build: &str, sink: Arc<dyn Sink>) -> io::Result<Client> {
        use tokio::net::windows::named_pipe::ClientOptions;
        const ERROR_PIPE_BUSY: i32 = 231;
        let deadline = Instant::now() + Duration::from_secs(1);
        let pipe = loop {
            match ClientOptions::new().open(name) {
                Ok(p) => break p,
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => return Err(e),
            }
        };
        crate::pipe::check_server_owner(&pipe)?;
        Client::over(pipe, role, build, sink).await
    }

    /// Say hello over any stream; `connect` and the tests use it.
    pub async fn over<S>(stream: S, role: &str, build: &str, sink: Arc<dyn Sink>) -> io::Result<Client>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (mut rd, mut wr) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let pending: Arc<Mutex<Option<Pending>>> = Arc::new(Mutex::new(Some(HashMap::new())));
        let (closed_tx, closed) = watch::channel(false);
        let quiet = Arc::new(AtomicBool::new(false));
        let (close, mut on_close) = oneshot::channel::<()>();

        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if wr.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = Arc::clone(&pending);
        let reader_quiet = Arc::clone(&quiet);
        tokio::spawn(async move {
            let mut decoder = Decoder::default();
            let mut buf = vec![0u8; 64 * 1024];
            let mut dropped = false;
            'read: loop {
                let n = tokio::select! {
                    r = rd.read(&mut buf) => match r {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    },
                    _ = &mut on_close => {
                        dropped = true;
                        break;
                    }
                };
                decoder.push(&buf[..n]);
                loop {
                    match decoder.next_frame() {
                        Ok(Some(Frame::Control(json))) => match protocol::parse::<DaemonMsg>(&json) {
                            Ok(DaemonMsg::Reply { id, result }) => {
                                let waiter = reader_pending.lock().unwrap().as_mut().and_then(|p| p.remove(&id));
                                if let Some(w) = waiter {
                                    let _ = w.send(result);
                                }
                            }
                            Ok(event) => sink.event(event),
                            Err(e) => {
                                vlog!("[client] {e}");
                                break 'read;
                            }
                        },
                        Ok(Some(Frame::Output { session, bytes })) => sink.output(session, bytes),
                        Ok(Some(Frame::Snapshot { session, bytes })) => sink.snapshot(session, bytes),
                        Ok(Some(Frame::Input { .. })) => {
                            vlog!("[client] daemon sent input");
                            break 'read;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            vlog!("[client] {e}");
                            break 'read;
                        }
                    }
                }
            }
            // Dropping the waiters fails every pending request.
            reader_pending.lock().unwrap().take();
            let _ = closed_tx.send(true);
            if !dropped && !reader_quiet.load(Ordering::SeqCst) {
                sink.disconnected();
            }
        });

        let mut client = Client {
            tx,
            pending,
            next: AtomicU64::new(1),
            closed,
            quiet,
            _close: close,
            daemon: DaemonInfo { protocol: 0, build: String::new(), sessions: 0 },
        };
        let hello = ClientMsg::Hello { protocol: PROTOCOL, build: build.into(), role: role.into() };
        match client.request(hello).await {
            Ok(ReplyBody::Hello { protocol, build, sessions }) => {
                client.daemon = DaemonInfo { protocol, build, sessions };
                Ok(client)
            }
            Ok(other) => Err(io::Error::new(io::ErrorKind::InvalidData, format!("hello got {other:?}"))),
            Err(e) => Err(io::Error::new(io::ErrorKind::ConnectionAborted, e)),
        }
    }

    /// Send and wait for the reply. An error is the daemon's message, or
    /// `DISCONNECTED`.
    pub async fn request(&self, msg: ClientMsg) -> Result<ReplyBody, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        match self.pending.lock().unwrap().as_mut() {
            Some(p) => p.insert(id, tx),
            None => return Err(DISCONNECTED.into()),
        };
        self.raw(Frame::control(&Request { id, msg }));
        rx.await.unwrap_or_else(|_| Err(DISCONNECTED.into()))
    }

    /// Send with no reply wanted.
    pub fn send(&self, msg: ClientMsg) {
        self.raw(Frame::control(&Request { id: 0, msg }));
    }

    /// Bytes for a session's child.
    pub fn input(&self, session: u32, bytes: Vec<u8>) {
        self.raw(Frame::Input { session, bytes });
    }

    fn raw(&self, frame: Frame) {
        let _ = self.tx.send(frame.encode());
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Don't report this client's disconnect: it is being closed on purpose.
    pub fn quiet(&self) {
        self.quiet.store(true, Ordering::SeqCst);
    }

    /// End every shell and the daemon, and wait up to `timeout` for the pipe
    /// to close. The disconnect that follows is not reported.
    pub async fn shutdown(&self, timeout: Duration) {
        self.quiet();
        let _ = tokio::time::timeout(timeout, async {
            let _ = self.request(ClientMsg::Shutdown).await;
            let mut closed = self.closed.clone();
            let _ = closed.wait_for(|c| *c).await;
        })
        .await;
    }
}

// ---- starting the daemon (plan-19 5.7) ----

#[derive(Debug)]
pub enum StartError {
    /// A daemon of another protocol has sessions; ending it ends them.
    Mismatch { sessions: usize },
    Io(io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Mismatch { sessions } => {
                write!(f, "{sessions} sessions are running on an older background service")
            }
            StartError::Io(e) => write!(f, "background service: {e}"),
        }
    }
}

/// Which daemon to launch and what it is called: next to `app_exe` in a
/// release build. A debug build launches a copy named after the build, since a
/// running daemon locks its own file and the next `cargo build` would fail
/// (plan-19 D6).
pub fn daemon_exe(app_exe: &Path, debug: bool) -> io::Result<(PathBuf, String)> {
    let src = app_exe.with_file_name("veal-daemon.exe");
    if !debug {
        return Ok((src, format!("release-{}", env!("CARGO_PKG_VERSION"))));
    }
    let meta = std::fs::metadata(&src)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let build = format!("{:x}-{:x}", meta.len(), mtime);
    let copy = app_exe.with_file_name(format!("veal-daemon-{build}.exe"));
    if !copy.is_file() {
        std::fs::copy(&src, &copy)?;
    }
    // Old copies are dead weight; a running one is locked and stays.
    if let Some(dir) = app_exe.parent() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("veal-daemon-") && name.ends_with(".exe") && e.path() != copy {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    Ok((copy, build))
}

/// Start the daemon so that it outlives the app: out of the app's job if the
/// job allows it, with no console, and inheriting no handles. `Command` would
/// pass on every inheritable handle, such as a dev terminal's output pipe,
/// which a long-lived daemon would then hold open.
#[cfg(windows)]
pub fn launch(exe: &Path, build: &str, pipe: &str) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS, PROCESS_INFORMATION,
        STARTUPINFOW,
    };
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let quote = |s: &str| format!("\"{}\"", s.replace('"', ""));
    let line = format!("{} --build {} --pipe {}", quote(&exe.to_string_lossy()), quote(build), quote(pipe));
    let app = wide(exe.as_os_str());
    // Not the app's cwd, which the daemon would then hold open.
    let dir = exe.parent().map(|d| wide(d.as_os_str()));
    let start = |flags: u32| -> io::Result<()> {
        let mut cmd: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessW(
                app.as_ptr(),
                cmd.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                flags,
                std::ptr::null(),
                dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
                &si,
                &mut pi,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
        }
        Ok(())
    };
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    match start(base | CREATE_BREAKAWAY_FROM_JOB) {
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            vlog!("[client] job forbids breakaway; daemon starts inside it");
            start(base)
        }
        other => other,
    }
}

/// Connect to the daemon, starting it if there is none. In a debug build a
/// daemon of another build with no sessions is replaced, so the dev loop
/// picks up daemon changes; one with sessions is kept, so a restart does not
/// end them.
#[cfg(windows)]
pub async fn connect_or_launch(
    name: &str,
    exe: &Path,
    build: &str,
    debug: bool,
    sink: Arc<dyn Sink>,
) -> Result<Client, StartError> {
    let role = "desktop";
    let mut launched = false;
    for _ in 0..2 {
        let client = match Client::connect(name, role, build, Arc::clone(&sink)).await {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                launch(exe, build, name).map_err(StartError::Io)?;
                launched = true;
                wait_connect(name, role, build, &sink).await?
            }
            Err(e) => return Err(StartError::Io(e)),
        };
        let d = &client.daemon;
        let same_protocol = d.protocol == PROTOCOL;
        let same_build = d.build == build;
        if same_protocol && (same_build || !debug) {
            return Ok(client);
        }
        if d.sessions > 0 {
            if same_protocol {
                vlog!("[client] keeping daemon build {} with {} sessions", d.build, d.sessions);
                return Ok(client);
            }
            client.quiet();
            return Err(StartError::Mismatch { sessions: d.sessions });
        }
        if launched {
            // We just started it and it still disagrees: use it rather than loop.
            if same_protocol {
                return Ok(client);
            }
            return Err(StartError::Mismatch { sessions: 0 });
        }
        vlog!("[client] replacing idle daemon build {} with {build}", d.build);
        client.shutdown(Duration::from_secs(3)).await;
        wait_gone(name).await;
    }
    Err(StartError::Io(io::Error::other("daemon would not start")))
}

#[cfg(windows)]
async fn wait_connect(name: &str, role: &str, build: &str, sink: &Arc<dyn Sink>) -> Result<Client, StartError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match Client::connect(name, role, build, Arc::clone(sink)).await {
            Ok(c) => return Ok(c),
            Err(e) if Instant::now() >= deadline => return Err(StartError::Io(e)),
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}

/// Until nothing serves `name`, for up to 3 s.
#[cfg(windows)]
async fn wait_gone(name: &str) {
    use tokio::net::windows::named_pipe::ClientOptions;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match ClientOptions::new().open(name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return,
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Collect {
        out: Mutex<Vec<(u32, Vec<u8>, bool)>>,
        events: Mutex<Vec<DaemonMsg>>,
        gone: AtomicBool,
    }
    impl Sink for Collect {
        fn output(&self, s: u32, b: Vec<u8>) {
            self.out.lock().unwrap().push((s, b, false));
        }
        fn snapshot(&self, s: u32, b: Vec<u8>) {
            self.out.lock().unwrap().push((s, b, true));
        }
        fn event(&self, m: DaemonMsg) {
            self.events.lock().unwrap().push(m);
        }
        fn disconnected(&self) {
            self.gone.store(true, Ordering::SeqCst);
        }
    }

    async fn read_request(io: &mut tokio::io::DuplexStream, d: &mut Decoder) -> Request {
        let mut buf = [0u8; 4096];
        loop {
            if let Some(Frame::Control(json)) = d.next_frame().unwrap() {
                return protocol::parse(&json).unwrap();
            }
            let n = io.read(&mut buf).await.unwrap();
            d.push(&buf[..n]);
        }
    }

    async fn send(io: &mut tokio::io::DuplexStream, msg: &DaemonMsg) {
        io.write_all(&Frame::control(msg).encode()).await.unwrap();
    }

    #[tokio::test]
    async fn replies_out_of_order_reach_their_callers_and_close_fails_the_rest() {
        let (a, mut b) = tokio::io::duplex(1 << 16);
        let sink = Arc::new(Collect::default());
        let server = tokio::spawn(async move {
            let mut d = Decoder::default();
            let hello = read_request(&mut b, &mut d).await;
            let body = ReplyBody::Hello { protocol: PROTOCOL, build: "x".into(), sessions: 0 };
            send(&mut b, &DaemonMsg::Reply { id: hello.id, result: Ok(body) }).await;
            let first = read_request(&mut b, &mut d).await;
            let second = read_request(&mut b, &mut d).await;
            let _third = read_request(&mut b, &mut d).await;
            // Output and an event in between, then the replies backwards.
            b.write_all(&Frame::Output { session: 9, bytes: b"hi".to_vec() }.encode()).await.unwrap();
            send(&mut b, &DaemonMsg::Exit { key: "k".into(), code: Some(3) }).await;
            send(&mut b, &DaemonMsg::Reply { id: second.id, result: Err("two".into()) }).await;
            send(&mut b, &DaemonMsg::Reply { id: first.id, result: Ok(ReplyBody::Ok) }).await;
            // The third never gets an answer: the pipe closes.
        });
        let client = Arc::new(Client::over(a, "test", "x", sink.clone() as Arc<dyn Sink>).await.unwrap());
        assert_eq!(client.daemon.build, "x");
        let c1 = Arc::clone(&client);
        let c2 = Arc::clone(&client);
        let c3 = Arc::clone(&client);
        let r1 = tokio::spawn(async move { c1.request(ClientMsg::List).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let r2 = tokio::spawn(async move { c2.request(ClientMsg::List).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let r3 = tokio::spawn(async move { c3.request(ClientMsg::List).await });
        assert_eq!(r1.await.unwrap(), Ok(ReplyBody::Ok));
        assert_eq!(r2.await.unwrap(), Err("two".into()));
        server.await.unwrap();
        assert_eq!(r3.await.unwrap(), Err(DISCONNECTED.into()));
        assert!(sink.gone.load(Ordering::SeqCst));
        assert_eq!(sink.out.lock().unwrap().as_slice(), &[(9, b"hi".to_vec(), false)]);
        assert_eq!(sink.events.lock().unwrap().len(), 1);
        assert_eq!(client.request(ClientMsg::List).await, Err(DISCONNECTED.into()));
    }
}
