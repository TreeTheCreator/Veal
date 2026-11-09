//! What the app and the daemon say to each other (plan-19 section 4). Pure:
//! no I/O, so the same frames can later go over a WebSocket (plan-20).
//!
//! A frame is `u32 len` (little-endian, counting the kind byte and the
//! payload), `u8 kind`, then the payload. Control frames carry JSON. Output,
//! snapshot and input frames carry `u32 session` and then raw bytes, so a
//! chunk of output or a paste is never escaped.

use serde::{Deserialize, Serialize};

/// Bumped whenever a message changes shape.
pub const PROTOCOL: u32 = 1;

/// A length past this is an error before anything is allocated. A snapshot
/// of 5000 lines is about 330 KB, and a queue is capped at 1 MB.
pub const MAX_FRAME: u32 = 16 * 1024 * 1024;

const KIND_CONTROL: u8 = 1;
const KIND_OUTPUT: u8 = 2;
const KIND_SNAPSHOT: u8 = 3;
const KIND_INPUT: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// JSON: a `Request` from a client, a `DaemonMsg` from the daemon.
    Control(Vec<u8>),
    /// Daemon to client: bytes that follow what the client already has.
    Output { session: u32, bytes: Vec<u8> },
    /// Daemon to client: a whole screen, starting with a reset (plan-19 D8).
    Snapshot { session: u32, bytes: Vec<u8> },
    /// Client to daemon: bytes for the child, written whole.
    Input { session: u32, bytes: Vec<u8> },
}

#[derive(Debug, PartialEq)]
pub enum ProtoError {
    TooLong(u32),
    Empty,
    UnknownKind(u8),
    Short,
    Json(String),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtoError::TooLong(n) => write!(f, "frame of {n} bytes is over the limit"),
            ProtoError::Empty => write!(f, "frame with no kind"),
            ProtoError::UnknownKind(k) => write!(f, "unknown frame kind {k}"),
            ProtoError::Short => write!(f, "binary frame shorter than its session number"),
            ProtoError::Json(e) => write!(f, "bad control message: {e}"),
        }
    }
}

impl std::error::Error for ProtoError {}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let (kind, session, bytes) = match self {
            Frame::Control(b) => (KIND_CONTROL, None, b),
            Frame::Output { session, bytes } => (KIND_OUTPUT, Some(*session), bytes),
            Frame::Snapshot { session, bytes } => (KIND_SNAPSHOT, Some(*session), bytes),
            Frame::Input { session, bytes } => (KIND_INPUT, Some(*session), bytes),
        };
        let len = 1 + session.map_or(0, |_| 4) + bytes.len();
        let mut out = Vec::with_capacity(4 + len);
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.push(kind);
        if let Some(s) = session {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out.extend_from_slice(bytes);
        out
    }

    pub fn control<T: Serialize>(msg: &T) -> Frame {
        Frame::Control(serde_json::to_vec(msg).expect("protocol types always serialize"))
    }
}

/// Parses a control frame's JSON.
pub fn parse<T: for<'de> Deserialize<'de>>(json: &[u8]) -> Result<T, ProtoError> {
    serde_json::from_slice(json).map_err(|e| ProtoError::Json(e.to_string()))
}

/// Bytes in, frames out, however the bytes were split.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    start: usize,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        if self.start > 0 && self.start == self.buf.len() {
            self.buf.clear();
            self.start = 0;
        } else if self.start > 64 * 1024 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// The next whole frame, `None` if more bytes are needed. After an error
    /// the stream cannot be trusted and the connection should close.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtoError> {
        let rest = &self.buf[self.start..];
        if rest.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_le_bytes(rest[..4].try_into().unwrap());
        if len > MAX_FRAME {
            return Err(ProtoError::TooLong(len));
        }
        if len == 0 {
            return Err(ProtoError::Empty);
        }
        let len = len as usize;
        if rest.len() < 4 + len {
            return Ok(None);
        }
        let kind = rest[4];
        let body = &rest[5..4 + len];
        let frame = match kind {
            KIND_CONTROL => Frame::Control(body.to_vec()),
            KIND_OUTPUT | KIND_SNAPSHOT | KIND_INPUT => {
                if body.len() < 4 {
                    return Err(ProtoError::Short);
                }
                let session = u32::from_le_bytes(body[..4].try_into().unwrap());
                let bytes = body[4..].to_vec();
                match kind {
                    KIND_OUTPUT => Frame::Output { session, bytes },
                    KIND_SNAPSHOT => Frame::Snapshot { session, bytes },
                    _ => Frame::Input { session, bytes },
                }
            }
            k => return Err(ProtoError::UnknownKind(k)),
        };
        self.start += 4 + len;
        Ok(Some(frame))
    }
}

/// One row of `list`: what is running or has exited, by key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub key: String,
    pub cwd: String,
    pub exited: bool,
    pub code: Option<i32>,
}

/// A client's control message, with the id its reply will echo. Id 0 asks
/// for no reply (plan-19 D1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub msg: ClientMsg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// First message on a connection. `build` identifies the binary in debug.
    Hello { protocol: u32, build: String, role: String },
    List,
    /// Attach to `key`, or start it and attach (plan-19 D7).
    Spawn {
        key: String,
        cwd: String,
        cols: u16,
        rows: u16,
        shell: Option<String>,
        proxy_mode: Option<String>,
        proxy_port: Option<String>,
        proxy_bypass: Option<String>,
        /// Set after the fresh user environment; dev builds send their own.
        env: Option<Vec<(String, String)>>,
        responder: bool,
        controller: bool,
    },
    /// Attach to `key`, never spawn.
    Attach { key: String, cols: u16, rows: u16, responder: bool, controller: bool },
    Detach { key: String },
    Resize { key: String, cols: u16, rows: u16 },
    Kill { key: String },
    Forget { key: String },
    /// Drop this client's queue; its next frame for `key` is a snapshot.
    Resync { key: String },
    /// For OSC 10/11 answers while no responder is attached.
    Theme { fg: (u8, u8, u8), bg: (u8, u8, u8) },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ReplyBody {
    Hello { protocol: u32, build: String, sessions: usize },
    List { sessions: Vec<SessionInfo> },
    /// `session` is the number output frames carry for this key.
    Opened { session: u32, exited: bool, code: Option<i32>, agent: Option<String> },
    Ok,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum DaemonMsg {
    Reply { id: u64, result: Result<ReplyBody, String> },
    Exit { key: String, code: Option<i32> },
    Agent { key: String, agent: Option<String> },
    SessionAdded { key: String },
    SessionRemoved { key: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(bytes: &[u8]) -> Vec<Frame> {
        let mut d = Decoder::default();
        d.push(bytes);
        let mut out = Vec::new();
        while let Some(f) = d.next_frame().unwrap() {
            out.push(f);
        }
        out
    }

    #[test]
    fn frames_round_trip() {
        let frames = vec![
            Frame::Control(b"{}".to_vec()),
            Frame::Output { session: 7, bytes: b"hi\x1b[0m".to_vec() },
            Frame::Snapshot { session: u32::MAX, bytes: vec![] },
            Frame::Input { session: 1, bytes: vec![0, 255, 10] },
        ];
        let wire: Vec<u8> = frames.iter().flat_map(|f| f.encode()).collect();
        assert_eq!(decode_all(&wire), frames);
    }

    #[test]
    fn one_byte_at_a_time() {
        let f = Frame::Output { session: 3, bytes: b"hello".to_vec() };
        let wire = f.encode();
        let mut d = Decoder::default();
        for (i, b) in wire.iter().enumerate() {
            d.push(std::slice::from_ref(b));
            let got = d.next_frame().unwrap();
            if i + 1 < wire.len() {
                assert_eq!(got, None, "at byte {i}");
            } else {
                assert_eq!(got, Some(f.clone()));
            }
        }
        assert_eq!(d.next_frame().unwrap(), None);
    }

    #[test]
    fn bad_frames_are_errors() {
        let mut d = Decoder::default();
        d.push(&(MAX_FRAME + 1).to_le_bytes());
        assert_eq!(d.next_frame(), Err(ProtoError::TooLong(MAX_FRAME + 1)));

        let mut d = Decoder::default();
        d.push(&0u32.to_le_bytes());
        assert_eq!(d.next_frame(), Err(ProtoError::Empty));

        let mut d = Decoder::default();
        d.push(&[1, 0, 0, 0, 9]);
        assert_eq!(d.next_frame(), Err(ProtoError::UnknownKind(9)));

        let mut d = Decoder::default();
        d.push(&[3, 0, 0, 0, KIND_OUTPUT, 1, 2]);
        assert_eq!(d.next_frame(), Err(ProtoError::Short));

        assert!(matches!(parse::<Request>(b"{nope"), Err(ProtoError::Json(_))));
        assert!(matches!(parse::<Request>(br#"{"id":1,"t":"launch_missiles"}"#), Err(ProtoError::Json(_))));
    }

    #[test]
    fn random_bytes_never_panic() {
        // A small xorshift, so the test is deterministic.
        let mut x: u64 = 0x9e3779b97f4a7c15;
        for _ in 0..2000 {
            let mut d = Decoder::default();
            let mut bytes = Vec::new();
            for _ in 0..(x % 64) {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                bytes.push(x as u8);
            }
            // Keep lengths small so a valid-looking prefix is reachable.
            if bytes.len() > 4 {
                bytes[2] = 0;
                bytes[3] = 0;
            }
            d.push(&bytes);
            while let Ok(Some(Frame::Control(json))) = d.next_frame() {
                let _ = parse::<Request>(&json);
                let _ = parse::<DaemonMsg>(&json);
            }
            x = x.wrapping_add(1);
        }
    }

    #[test]
    fn messages_round_trip() {
        let requests = vec![
            ClientMsg::Hello { protocol: PROTOCOL, build: "b".into(), role: "desktop".into() },
            ClientMsg::List,
            ClientMsg::Spawn {
                key: "k".into(),
                cwd: "C:\\".into(),
                cols: 80,
                rows: 24,
                shell: None,
                proxy_mode: Some("system".into()),
                proxy_port: None,
                proxy_bypass: Some("".into()),
                env: Some(vec![("A".into(), "1".into())]),
                responder: true,
                controller: true,
            },
            ClientMsg::Attach { key: "k".into(), cols: 1, rows: 2, responder: false, controller: false },
            ClientMsg::Detach { key: "k".into() },
            ClientMsg::Resize { key: "k".into(), cols: 3, rows: 4 },
            ClientMsg::Kill { key: "k".into() },
            ClientMsg::Forget { key: "k".into() },
            ClientMsg::Resync { key: "k".into() },
            ClientMsg::Theme { fg: (1, 2, 3), bg: (4, 5, 6) },
            ClientMsg::Shutdown,
        ];
        for (i, msg) in requests.into_iter().enumerate() {
            let req = Request { id: i as u64, msg };
            let Frame::Control(json) = Frame::control(&req) else { unreachable!() };
            assert_eq!(parse::<Request>(&json).unwrap(), req);
        }
        let replies = vec![
            DaemonMsg::Reply { id: 1, result: Ok(ReplyBody::Hello { protocol: 1, build: "b".into(), sessions: 2 }) },
            DaemonMsg::Reply {
                id: 2,
                result: Ok(ReplyBody::List {
                    sessions: vec![SessionInfo { key: "k".into(), cwd: "c".into(), exited: true, code: Some(3) }],
                }),
            },
            DaemonMsg::Reply {
                id: 3,
                result: Ok(ReplyBody::Opened { session: 4, exited: false, code: None, agent: Some("claude".into()) }),
            },
            DaemonMsg::Reply { id: 4, result: Ok(ReplyBody::Ok) },
            DaemonMsg::Reply { id: 5, result: Err("already running 4 terminals".into()) },
            DaemonMsg::Exit { key: "k".into(), code: None },
            DaemonMsg::Agent { key: "k".into(), agent: None },
            DaemonMsg::SessionAdded { key: "k".into() },
            DaemonMsg::SessionRemoved { key: "k".into() },
        ];
        for msg in replies {
            let Frame::Control(json) = Frame::control(&msg) else { unreachable!() };
            assert_eq!(parse::<DaemonMsg>(&json).unwrap(), msg);
        }
    }
}
