//! A real `veal-daemon.exe` on a pipe of its own per test, driven by the
//! app's client (plan-19 5.8).
#![cfg(windows)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::ClientOptions;
use veal_core::client::{connect_or_launch, replace, Client, Sink, StartError};
use veal_core::protocol::{ClientMsg, DaemonMsg, Frame, ReplyBody, Request, PROTOCOL};

#[derive(Default)]
struct Screen {
    bytes: Mutex<HashMap<u32, Vec<u8>>>,
    snapshots: Mutex<HashMap<u32, usize>>,
    events: Mutex<Vec<DaemonMsg>>,
    gone: AtomicBool,
}

impl Sink for Screen {
    fn output(&self, s: u32, b: Vec<u8>) {
        self.bytes.lock().unwrap().entry(s).or_default().extend(b);
    }
    fn snapshot(&self, s: u32, b: Vec<u8>) {
        *self.snapshots.lock().unwrap().entry(s).or_default() += 1;
        self.bytes.lock().unwrap().entry(s).or_default().extend(b);
    }
    fn event(&self, m: DaemonMsg) {
        self.events.lock().unwrap().push(m);
    }
    fn disconnected(&self) {
        self.gone.store(true, Ordering::SeqCst);
    }
}

impl Screen {
    fn text(&self, s: u32) -> String {
        String::from_utf8_lossy(self.bytes.lock().unwrap().get(&s).map_or(&[][..], |v| v)).into_owned()
    }

    async fn wait_for(&self, s: u32, needle: &str, secs: u64) {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while !self.text(s).contains(needle) {
            if Instant::now() >= deadline {
                let text = self.text(s);
                let tail: String = text.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
                panic!("no {needle:?} in {} bytes, ending {tail:?}", text.len());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_veal-daemon"))
}

/// Shuts the test's daemon down when the test ends, passed or not, so a
/// failure does not leave a daemon holding the exe open for the next build.
struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let name = self.0.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let sink = Arc::new(Screen::default()) as Arc<dyn Sink>;
                if let Ok(c) = Client::connect(&name, "cleanup", "test", sink).await {
                    c.shutdown(Duration::from_secs(3)).await;
                }
            });
        })
        .join();
    }
}

fn unique(tag: &str) -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!(r"\\.\pipe\veal-it-{tag}-{}-{n}", std::process::id())
}

async fn start(name: &str) -> (Client, Arc<Screen>) {
    let screen = Arc::new(Screen::default());
    let client = connect_or_launch(name, &exe(), "test", false, Arc::clone(&screen) as Arc<dyn Sink>)
        .await
        .expect("daemon");
    (client, screen)
}

fn spawn_msg(key: &str) -> ClientMsg {
    ClientMsg::Spawn {
        key: key.into(),
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        cols: 80,
        rows: 24,
        shell: Some("cmd.exe".into()),
        proxy_mode: Some("off".into()),
        proxy_port: None,
        proxy_bypass: None,
        env: None,
        responder: true,
        controller: true,
    }
}

fn attach_msg(key: &str) -> ClientMsg {
    ClientMsg::Attach { key: key.into(), cols: 80, rows: 24, responder: true, controller: true }
}

fn opened(r: Result<ReplyBody, String>) -> (u32, u32) {
    match r {
        Ok(ReplyBody::Opened { session, client, .. }) => (session, client),
        other => panic!("{other:?}"),
    }
}

/// Answer ConPTY's opening query as xterm does, then wait for the prompt.
async fn ready(client: &Client, screen: &Screen, s: u32) {
    screen.wait_for(s, "\x1b[6n", 10).await;
    client.input(s, b"\x1b[1;1R".to_vec());
    screen.wait_for(s, ">", 10).await;
}

async fn pipe_gone(name: &str, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Err(e) = ClientOptions::new().open(name) {
            if e.kind() == std::io::ErrorKind::NotFound {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn reattach_restores_the_screen() {
    let name = unique("reattach");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("a")).await);
    ready(&c, &screen, s).await;
    c.input(s, b"echo marker-1234\r\n".to_vec());
    screen.wait_for(s, "marker-1234\r\n", 10).await;
    drop(c);

    let (c2, screen2) = start(&name).await;
    assert_eq!(c2.daemon.sessions, 1);
    let (s2, _) = opened(c2.request(attach_msg("a")).await);
    assert_eq!(s2, s, "the same session");
    screen2.wait_for(s2, "marker-1234", 10).await;
    assert_eq!(screen2.snapshots.lock().unwrap().get(&s2), Some(&1));
    // And it still runs commands.
    c2.input(s2, b"echo after-5678\r\n".to_vec());
    screen2.wait_for(s2, "after-5678\r\n", 10).await;
    c2.shutdown(Duration::from_secs(3)).await;
}

/// Ctrl+C reaches the pane's foreground program. A daemon started in its own
/// process group would pass on "Ctrl+C disabled" to every shell.
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_stops_the_foreground_program() {
    let name = unique("ctrlc");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("c")).await);
    ready(&c, &screen, s).await;
    c.input(s, b"ping -t 127.0.0.1\r\n".to_vec());
    screen.wait_for(s, "Reply from", 10).await;
    c.input(s, vec![3]);
    c.input(s, b"set /a 70+7\r\n".to_vec());
    // `77` is only in the output, never in the echoed input.
    screen.wait_for(s, "77", 10).await;
    c.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_is_idempotent_across_connections() {
    let name = unique("idem");
    let _cleanup = Cleanup(name.clone());
    let (c1, _) = start(&name).await;
    let (c2, _) = start(&name).await;
    let (a, _) = opened(c1.request(spawn_msg("same")).await);
    let (b, _) = opened(c2.request(spawn_msg("same")).await);
    assert_eq!(a, b);
    match c1.request(ClientMsg::List).await {
        Ok(ReplyBody::List { sessions }) => assert_eq!(sessions.len(), 1),
        other => panic!("{other:?}"),
    }
    c1.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fifth_session_is_refused() {
    let name = unique("cap");
    let _cleanup = Cleanup(name.clone());
    let (c, _) = start(&name).await;
    for k in ["1", "2", "3", "4"] {
        opened(c.request(spawn_msg(k)).await);
    }
    let err = c.request(spawn_msg("5")).await.unwrap_err();
    assert!(err.contains("already running"), "{err}");
    c.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exit_while_detached_is_a_tombstone() {
    let name = unique("tomb");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("t")).await);
    ready(&c, &screen, s).await;
    c.input(s, b"ping -n 2 127.0.0.1 >nul & exit 3\r\n".to_vec());
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(c);
    tokio::time::sleep(Duration::from_secs(3)).await;

    let (c2, _) = start(&name).await;
    match c2.request(attach_msg("t")).await {
        Ok(ReplyBody::Opened { exited, code, .. }) => assert_eq!((exited, code), (true, Some(3))),
        other => panic!("{other:?}"),
    }
    c2.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resync_gives_a_snapshot() {
    let name = unique("resync");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("r")).await);
    ready(&c, &screen, s).await;
    c.request(ClientMsg::Resync { key: "r".into() }).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while screen.snapshots.lock().unwrap().get(&s).is_none() {
        assert!(Instant::now() < deadline, "no snapshot");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    c.shutdown(Duration::from_secs(3)).await;
}

/// Time for `lines` lines of `cmd` output to reach this client.
async fn flood(c: &Client, screen: &Screen, s: u32, tag: &str, lines: u32) -> Duration {
    let t = Instant::now();
    c.input(s, format!("for /L %i in (1,1,{lines}) do @echo {tag}-%i\r\n").into_bytes());
    // ConPTY puts a title change between the CR and the LF.
    screen.wait_for(s, &format!("{tag}-{lines}\r"), 120).await;
    t.elapsed()
}

/// A connection that never reads must not slow another client or the child.
/// Measured against the same flood with no stalled client: `cmd`'s echo is
/// slow on its own, about 300 lines a second through ConPTY.
#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_client_does_not_slow_the_others() {
    let name = unique("stall");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("flood")).await);
    ready(&c, &screen, s).await;
    let alone = flood(&c, &screen, s, "alone", 3000).await;

    // A raw connection: hello, attach, and then never read a byte.
    let mut stalled = ClientOptions::new().open(&name).unwrap();
    for (id, msg) in [
        (1, ClientMsg::Hello { protocol: PROTOCOL, build: "x".into(), role: "stalled".into() }),
        (2, attach_msg("flood")),
    ] {
        stalled.write_all(&Frame::control(&Request { id, msg }).encode()).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let with = flood(&c, &screen, s, "stalled", 3000).await;
    assert!(with < alone * 2 + Duration::from_secs(3), "alone {alone:?}, with a stalled client {with:?}");
    drop(stalled);
    c.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_frame_closes_only_its_connection() {
    let name = unique("bad");
    let _cleanup = Cleanup(name.clone());
    let (c, screen) = start(&name).await;
    let (s, _) = opened(c.request(spawn_msg("b")).await);
    ready(&c, &screen, s).await;

    let mut bad = ClientOptions::new().open(&name).unwrap();
    bad.write_all(&u32::MAX.to_le_bytes()).await.unwrap();
    bad.write_all(&[1]).await.unwrap();
    let mut buf = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(5), bad.read(&mut buf)).await.expect("closed in time");
    assert!(matches!(n, Ok(0) | Err(_)), "{n:?}");

    c.input(s, b"echo fine-42\r\n".to_vec());
    screen.wait_for(s, "fine-42\r\n", 10).await;
    assert!(!screen.gone.load(Ordering::SeqCst));
    c.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_ends_the_shells_and_the_daemon() {
    let name = unique("shutdown");
    let _cleanup = Cleanup(name.clone());
    let (c, _) = start(&name).await;
    opened(c.request(spawn_msg("x")).await);
    opened(c.request(spawn_msg("y")).await);
    c.shutdown(Duration::from_secs(3)).await;
    assert!(pipe_gone(&name, 3).await, "daemon still serving");
}

/// `veal-daemon.exe --stop --pipe <name>`, as the installer runs it.
async fn run_stop(name: &str) -> i32 {
    let name = name.to_string();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(exe())
            .args(["--stop", "--pipe", &name])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .code()
            .unwrap_or(-1)
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_flag_ends_a_running_daemon_and_its_shells() {
    let name = unique("stop");
    let _cleanup = Cleanup(name.clone());
    let (c, _) = start(&name).await;
    opened(c.request(spawn_msg("x")).await);
    assert_eq!(run_stop(&name).await, 0, "stopped");
    assert!(pipe_gone(&name, 3).await, "daemon still serving");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !c.is_closed() {
        assert!(Instant::now() < deadline, "the app's connection did not see it go");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(run_stop(&name).await, 1, "nothing left to stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_of_another_version_is_reported_then_replaced() {
    let name = unique("mismatch");
    let _cleanup = Cleanup(name.clone());
    let mut old = std::process::Command::new(exe())
        .args(["--pipe", &name, "--protocol", "99", "--build", "old"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Speak protocol 99 by hand, as an older app would, and start a session.
    let mut raw = None;
    for _ in 0..40 {
        if let Ok(p) = ClientOptions::new().open(&name) {
            raw = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut raw = raw.expect("old daemon listening");
    let hello = ClientMsg::Hello { protocol: 99, build: "old".into(), role: "desktop".into() };
    raw.write_all(&Frame::control(&Request { id: 1, msg: hello }).encode()).await.unwrap();
    raw.write_all(&Frame::control(&Request { id: 2, msg: spawn_msg("old") }).encode()).await.unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !String::from_utf8_lossy(&seen).contains("\"id\":2") {
        assert!(Instant::now() < deadline, "no spawn reply from the old daemon");
        let n = tokio::time::timeout(Duration::from_secs(10), raw.read(&mut buf)).await.unwrap().unwrap();
        seen.extend_from_slice(&buf[..n]);
    }
    drop(raw);

    let screen = Arc::new(Screen::default());
    let sink = || Arc::clone(&screen) as Arc<dyn Sink>;
    match connect_or_launch(&name, &exe(), "test", false, sink()).await {
        Err(StartError::Mismatch { sessions }) => assert_eq!(sessions, 1),
        Ok(_) => panic!("connected to a daemon of another protocol"),
        Err(e) => panic!("{e}"),
    }

    let c = replace(&name, &exe(), "test", false, sink()).await.expect("replaced");
    assert_eq!(c.daemon.protocol, PROTOCOL);
    assert!(old.wait().unwrap().success(), "the old daemon exited cleanly");
    match c.request(ClientMsg::List).await {
        Ok(ReplyBody::List { sessions }) => assert!(sessions.is_empty(), "{sessions:?}"),
        other => panic!("{other:?}"),
    }
    c.shutdown(Duration::from_secs(3)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_daemon_exits() {
    let name = unique("idle");
    let _cleanup = Cleanup(name.clone());
    let mut child = std::process::Command::new(exe())
        .args(["--pipe", &name, "--idle-ms", "500", "--build", "test"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let screen = Arc::new(Screen::default());
    let mut client = None;
    for _ in 0..40 {
        if let Ok(c) = Client::connect(&name, "test", "test", Arc::clone(&screen) as Arc<dyn Sink>).await {
            client = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let client = client.expect("connected");
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!client.is_closed(), "a connection keeps it alive");
    drop(client);
    assert!(pipe_gone(&name, 3).await, "idle daemon still running");
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "idle daemon still running");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_racing_starts_share_one_daemon() {
    let name = unique("race");
    let _cleanup = Cleanup(name.clone());
    let (n1, n2) = (name.clone(), name.clone());
    let a = tokio::spawn(async move { start(&n1).await });
    let b = tokio::spawn(async move { start(&n2).await });
    let ((c1, _), (c2, _)) = (a.await.unwrap(), b.await.unwrap());
    opened(c1.request(spawn_msg("shared")).await);
    match c2.request(ClientMsg::List).await {
        Ok(ReplyBody::List { sessions }) => assert_eq!(sessions.len(), 1, "same daemon"),
        other => panic!("{other:?}"),
    }
    c1.shutdown(Duration::from_secs(3)).await;
}

/// Needs a second local account; acceptance 14 checks it by hand.
#[test]
#[ignore]
fn another_users_pipe_is_refused() {}
