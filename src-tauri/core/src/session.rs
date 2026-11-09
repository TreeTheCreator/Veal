//! One shell's output after it leaves the PTY (plan-19, sections 2 and 3).
//!
//! Every byte the reader gets goes through an emulator and then onto a queue
//! per client, all under one lock. So a client that attaches gets a snapshot
//! that lines up exactly with the stream after it: no gap, no overlap.
//!
//! Nothing here blocks the reader, so the child is only ever limited by the
//! emulator. A client that falls `OUTBOX_CAP` behind loses its queue and gets a
//! fresh snapshot on its next take. Attaching is the same thing: a new client
//! starts out owing a snapshot, so there is one path that produces them.
//!
//! The serializer passed plan-19's gate; `scripts/gate` is how to re-run it.
//! Its rules are not obvious and each came from a failure there:
//! - Start with `ESC c ESC [ ? 25 h`: a reset leaves a hidden cursor hidden.
//! - End with whatever partial sequence the parser is holding (`Tail`), or the
//!   client prints the rest of it as text.
//! - `swap_alt` is the only way to the hidden main grid, and swapping back wipes
//!   the alternate grid, so that is cloned first and put back.
//! - A pending wrap only comes back by reprinting the last cell, and a column
//!   move after each non-ASCII glyph stops one width disagreement shifting a row.

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

/// Unfetched bytes a client may hold before it is dropped to a resync. An agent
/// redraw is tens of KB; this is seconds of the heaviest flood.
pub const OUTBOX_CAP: usize = 1024 * 1024;

/// Matches xterm's `scrollback` in TerminalPane.
pub const SCROLLBACK: usize = 5000;

/// What a client that has lost its place gets once the emulator is gone.
const RESET_ONLY: &[u8] = b"\x1bc\x1b[?25h";

pub type ClientId = u32;

/// Runs when a client's queue goes from empty to having something to take.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

// ---- the partial sequence the parser is holding at a cut ----

#[derive(Default)]
struct Tail {
    state: u8, // 0 ground, 1 esc, 2 csi, 3 string, 4 string-esc, 5 utf8
    need: u8,
    bytes: Vec<u8>,
}

impl Tail {
    fn feed(&mut self, data: &[u8]) {
        for &c in data {
            self.byte(c);
        }
    }

    fn start(&mut self, c: u8, state: u8) {
        self.bytes.clear();
        self.bytes.push(c);
        self.state = state;
    }

    fn byte(&mut self, c: u8) {
        match self.state {
            0 => {
                if c == 0x1b {
                    self.start(c, 1);
                } else if (0xc2..0xf5).contains(&c) {
                    self.need = if c < 0xe0 { 1 } else if c < 0xf0 { 2 } else { 3 };
                    self.start(c, 5);
                }
            }
            5 => {
                if c & 0xc0 == 0x80 {
                    self.bytes.push(c);
                    self.need -= 1;
                    if self.need == 0 {
                        self.done();
                    }
                } else {
                    self.done();
                    self.byte(c);
                }
            }
            1 => {
                self.bytes.push(c);
                match c {
                    b'[' => self.state = 2,
                    b']' | b'P' | b'X' | b'^' | b'_' => self.state = 3,
                    0x20..=0x2f => {} // intermediates: stay, need a final
                    _ => self.done(),
                }
            }
            2 => {
                if c == 0x1b {
                    self.start(c, 1);
                    return;
                }
                self.bytes.push(c);
                if (0x40..=0x7e).contains(&c) || c == 0x18 || c == 0x1a {
                    self.done();
                }
            }
            3 => {
                self.bytes.push(c);
                if c == 0x07 || c == 0x18 || c == 0x1a {
                    self.done();
                } else if c == 0x1b {
                    self.state = 4;
                }
            }
            _ => {
                self.bytes.push(c);
                if c == b'\\' {
                    self.done();
                } else {
                    self.state = 3;
                }
            }
        }
    }

    fn done(&mut self) {
        self.state = 0;
        self.bytes.clear();
    }
}

// ---- what the emulator says back ----

#[derive(Default)]
struct Heard {
    title: Option<String>,
    /// Answers to queries, in order. Sent to the child only when no responder
    /// is attached (section 3); otherwise xterm answers and these are dropped.
    replies: Vec<u8>,
    /// `(fg, bg)` for OSC 10/11 with no responder: the last theme reported.
    theme: Option<(Rgb, Rgb)>,
}

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Heard>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let mut heard = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match event {
            Event::Title(t) => heard.title = Some(t),
            Event::ResetTitle => heard.title = None,
            Event::PtyWrite(reply) => heard.replies.extend_from_slice(reply.as_bytes()),
            Event::ColorRequest(index, format) => {
                let Some((fg, bg)) = heard.theme else { return };
                let rgb = if index == NamedColor::Foreground as usize {
                    fg
                } else if index == NamedColor::Background as usize {
                    bg
                } else {
                    return;
                };
                heard.replies.extend_from_slice(format(rgb).as_bytes());
            }
            _ => {}
        }
    }
}

struct Size(usize, usize);

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

/// The emulator refuses less than this on resize and misbehaves on it at
/// creation. A pane can briefly measure zero before it has a layout.
fn clamped(cols: u16, rows: u16) -> Size {
    Size(cols.max(2) as usize, rows.max(1) as usize)
}

// ---- the emulator and its serializer ----

struct Screen {
    term: Term<Listener>,
    parser: Processor,
    heard: Listener,
    tail: Tail,
    #[cfg(test)]
    explode: bool,
}

impl Screen {
    fn new(cols: u16, rows: u16) -> Self {
        let heard = Listener::default();
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        Screen {
            term: Term::new(config, &clamped(cols, rows), heard.clone()),
            parser: Processor::new(),
            heard,
            tail: Tail::default(),
            #[cfg(test)]
            explode: false,
        }
    }

    /// Returns the answers the emulator would give to any queries in `bytes`.
    fn advance(&mut self, bytes: &[u8]) -> Vec<u8> {
        #[cfg(test)]
        assert!(!self.explode, "test: emulator panic");
        self.parser.advance(&mut self.term, bytes);
        self.tail.feed(bytes);
        std::mem::take(&mut self.heard.0.lock().unwrap_or_else(|p| p.into_inner()).replies)
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(clamped(cols, rows));
    }

    fn snapshot(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 * 1024);
        out.extend_from_slice(RESET_ONLY);
        let mode = *self.term.mode();
        let saved_alt = mode.contains(TermMode::ALT_SCREEN).then(|| self.term.grid().clone());
        if saved_alt.is_some() {
            self.term.swap_alt();
        }
        {
            let g = self.term.grid();
            write_grid(&mut out, g, -(g.history_size() as i32));
            let s = g.saved_cursor.point;
            out.extend_from_slice(format!("\x1b[{};{}H\x1b7", s.line.0 + 1, s.column.0 + 1).as_bytes());
            place_cursor(&mut out, g);
        }
        if let Some(alt) = saved_alt {
            self.term.swap_alt();
            *self.term.grid_mut() = alt;
            out.extend_from_slice(b"\x1b[?1049h\x1b[H");
            write_grid(&mut out, self.term.grid(), 0);
            place_cursor(&mut out, self.term.grid());
        }
        // The current pen.
        out.extend_from_slice(sgr(&self.term.grid().cursor.template).1.as_bytes());
        // Modes last: wrap must be on while the rows above are drawn.
        for (flag, seq) in [
            (TermMode::APP_CURSOR, "\x1b[?1h"),
            (TermMode::APP_KEYPAD, "\x1b="),
            (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h"),
            (TermMode::MOUSE_DRAG, "\x1b[?1002h"),
            (TermMode::MOUSE_MOTION, "\x1b[?1003h"),
            (TermMode::UTF8_MOUSE, "\x1b[?1005h"),
            (TermMode::SGR_MOUSE, "\x1b[?1006h"),
            (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
            (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
            (TermMode::INSERT, "\x1b[4h"),
            (TermMode::LINE_FEED_NEW_LINE, "\x1b[20h"),
        ] {
            if mode.contains(flag) {
                out.extend_from_slice(seq.as_bytes());
            }
        }
        if !mode.contains(TermMode::LINE_WRAP) {
            out.extend_from_slice(b"\x1b[?7l");
        }
        if !mode.contains(TermMode::SHOW_CURSOR) {
            out.extend_from_slice(b"\x1b[?25l");
        }
        let style = self.term.cursor_style();
        let code = match (style.shape, style.blinking) {
            (CursorShape::Block, true) => Some(1),
            (CursorShape::Block, false) => None,
            (CursorShape::Underline, true) => Some(3),
            (CursorShape::Underline, false) => Some(4),
            (CursorShape::Beam, true) => Some(5),
            (CursorShape::Beam, false) => Some(6),
            _ => None,
        };
        if let Some(code) = code {
            out.extend_from_slice(format!("\x1b[{code} q").as_bytes());
        }
        if let Some(title) = self.heard.0.lock().unwrap_or_else(|p| p.into_inner()).title.clone() {
            out.extend_from_slice(format!("\x1b]0;{title}\x07").as_bytes());
        }
        out.extend_from_slice(&self.tail.bytes);
        out
    }
}

fn color(out: &mut String, c: Color, base: u32) {
    use std::fmt::Write;
    match c {
        Color::Named(NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor) => {}
        Color::Named(n) => {
            let i = n as u32;
            if i < 8 {
                write!(out, ";{}", base + i).unwrap();
            } else if i < 16 {
                write!(out, ";{}", base + 60 + i - 8).unwrap();
            }
        }
        Color::Indexed(i) => write!(out, ";{};5;{}", base + 8, i).unwrap(),
        Color::Spec(rgb) => write!(out, ";{};2;{};{};{}", base + 8, rgb.r, rgb.g, rgb.b).unwrap(),
    }
}

type PenKey = (Color, Color, u16, Option<Color>);

fn sgr(c: &Cell) -> (PenKey, String) {
    use std::fmt::Write;
    let f = c.flags;
    let mut s = String::from("\x1b[0");
    if f.contains(Flags::BOLD) {
        s.push_str(";1");
    }
    if f.contains(Flags::DIM) {
        s.push_str(";2");
    }
    if f.contains(Flags::ITALIC) {
        s.push_str(";3");
    }
    if f.contains(Flags::UNDERCURL) {
        s.push_str(";4:3");
    } else if f.contains(Flags::DOTTED_UNDERLINE) {
        s.push_str(";4:4");
    } else if f.contains(Flags::DASHED_UNDERLINE) {
        s.push_str(";4:5");
    } else if f.contains(Flags::DOUBLE_UNDERLINE) {
        s.push_str(";4:2");
    } else if f.contains(Flags::UNDERLINE) {
        s.push_str(";4");
    }
    if f.contains(Flags::INVERSE) {
        s.push_str(";7");
    }
    if f.contains(Flags::HIDDEN) {
        s.push_str(";8");
    }
    if f.contains(Flags::STRIKEOUT) {
        s.push_str(";9");
    }
    color(&mut s, c.fg, 30);
    color(&mut s, c.bg, 40);
    let underline = c.underline_color();
    match underline {
        Some(Color::Indexed(i)) => write!(s, ";58;5;{i}").unwrap(),
        Some(Color::Spec(r)) => write!(s, ";58;2;{};{};{}", r.r, r.g, r.b).unwrap(),
        _ => {}
    }
    s.push('m');
    let visible = f & !(Flags::WRAPLINE | Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    ((c.fg, c.bg, visible.bits(), underline), s)
}

fn is_blank(c: &Cell) -> bool {
    let visible = c.flags & !Flags::WRAPLINE;
    c.c == ' '
        && visible.is_empty()
        && c.bg == Color::Named(NamedColor::Background)
        && c.zerowidth().is_none()
        && c.hyperlink().is_none()
}

/// Every line of a grid from `first` to the bottom, as bytes that redraw it.
fn write_grid(out: &mut Vec<u8>, g: &Grid<Cell>, first: i32) {
    let cols = g.columns();
    let last_line = g.screen_lines() as i32 - 1;
    let mut pen: Option<PenKey> = None;
    let mut link: Option<String> = None;
    let mut buf = [0u8; 4];
    let wrap_flag = |li: i32| g[Line(li)][Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    let has_text = |li: i32| (0..cols).any(|c| !is_blank(&g[Line(li)][Column(c)]));
    for li in first..=last_line {
        let row = &g[Line(li)];
        let flagged = wrap_flag(li);
        // A row can keep its flag after the row below was erased (a full-screen
        // app repaints in place). Only printing moves past a wrap, so with
        // nothing below to print this needs a line break, or that row is lost.
        let wrapped = flagged && li < last_line && has_text(li + 1);
        let end = if flagged {
            cols
        } else {
            (0..cols).rev().find(|&c| !is_blank(&row[Column(c)])).map_or(0, |c| c + 1)
        };
        let mut col = 0;
        while col < end {
            let cell = &row[Column(col)];
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                col += 1;
                continue;
            }
            let (key, code) = sgr(cell);
            if pen != Some(key) {
                out.extend_from_slice(code.as_bytes());
                pen = Some(key);
            }
            let uri = cell.hyperlink().map(|h| h.uri().to_string());
            if uri != link {
                out.extend_from_slice(format!("\x1b]8;;{}\x1b\\", uri.as_deref().unwrap_or("")).as_bytes());
                link = uri;
            }
            out.extend_from_slice(cell.c.encode_utf8(&mut buf).as_bytes());
            if let Some(zerowidth) = cell.zerowidth() {
                for &ch in zerowidth {
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
            if !cell.c.is_ascii() && col + width < cols {
                out.extend_from_slice(format!("\x1b[{}G", col + width + 1).as_bytes());
            }
            col += width;
        }
        if link.is_some() {
            out.extend_from_slice(b"\x1b]8;;\x1b\\");
            link = None;
        }
        if li < last_line && !wrapped {
            if pen.is_some() {
                out.extend_from_slice(b"\x1b[0m");
                pen = None;
            }
            out.extend_from_slice(b"\r\n");
        }
    }
    if pen.is_some() {
        out.extend_from_slice(b"\x1b[0m");
    }
}

fn place_cursor(out: &mut Vec<u8>, g: &Grid<Cell>) {
    let p = g.cursor.point;
    out.extend_from_slice(format!("\x1b[{};{}H", p.line.0 + 1, p.column.0 + 1).as_bytes());
    if g.cursor.input_needs_wrap {
        let cell = &g[p.line][p.column];
        out.extend_from_slice(sgr(cell).1.as_bytes());
        let mut buf = [0u8; 4];
        out.extend_from_slice(cell.c.encode_utf8(&mut buf).as_bytes());
    }
}

/// Runs `f` on the emulator. A panic costs the session its snapshots, not its
/// shell (plan-19, section 1): the screen goes and the bytes keep flowing.
fn guarded<T>(screen: &mut Option<Screen>, f: impl FnOnce(&mut Screen) -> T) -> Option<T> {
    let s = screen.as_mut()?;
    match catch_unwind(AssertUnwindSafe(|| f(s))) {
        Ok(value) => Some(value),
        Err(_) => {
            *screen = None;
            None
        }
    }
}

// ---- the session ----

struct Client {
    queue: Vec<u8>,
    /// A signal is outstanding: the client will take, so no further one is
    /// needed until it has.
    notified: bool,
    /// The queue is no use: the next take is a snapshot instead. New clients
    /// start like this, and a client over the cap ends up like this.
    resync: bool,
    responder: bool,
    notify: Notify,
}

#[derive(Default)]
struct Clients {
    next: ClientId,
    all: HashMap<ClientId, Client>,
}

struct Inner {
    /// `None` once the emulator has panicked, or the session is closed.
    screen: Option<Screen>,
    clients: Clients,
    /// The child exited, with its code if that could be read.
    exit: Option<Option<i32>>,
    /// The reader hit EOF; nothing more will be fed.
    eof: bool,
    /// Nobody wants this session any more: discard everything.
    closed: bool,
    /// Answers to a query left to the responder, with no output since. A child
    /// that went on printing has been answered or is not waiting. ConPTY waits
    /// for its opening query for ever, so if the responder goes first the answer
    /// must still be sent.
    owed: Vec<u8>,
}

pub struct Attached {
    pub client: ClientId,
    /// Set if the child had already exited: a tombstone.
    pub exit: Option<Option<i32>>,
}

pub struct Taken {
    pub bytes: Vec<u8>,
    /// `bytes` is a whole screen starting with a reset, not a continuation.
    pub snapshot: bool,
    /// This client is finished, and no client is left. The session can go.
    pub finished: bool,
}

pub struct Session {
    inner: Mutex<Inner>,
}

/// `ESC [ row ; col R` anywhere in `data`.
fn has_cursor_report(data: &[u8]) -> bool {
    data.windows(2).enumerate().any(|(i, w)| {
        if w != b"\x1b[" {
            return false;
        }
        let rest = &data[i + 2..];
        let row = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if row == 0 || rest.get(row) != Some(&b';') {
            return false;
        }
        let col = rest[row + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        col > 0 && rest.get(row + 1 + col) == Some(&b'R')
    })
}

impl Session {
    pub fn new(cols: u16, rows: u16) -> Self {
        Session {
            inner: Mutex::new(Inner {
                screen: Some(Screen::new(cols, rows)),
                clients: Clients::default(),
                exit: None,
                eof: false,
                closed: false,
                owed: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The reader's one step: emulator, then every queue, under one lock.
    /// Never blocks. Returns what must be written to the child, which the
    /// caller has to do off this thread: the child may not be reading.
    pub fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let mut inner = self.lock();
        if inner.closed {
            return Vec::new();
        }
        let replies = guarded(&mut inner.screen, |s| s.advance(bytes)).unwrap_or_default();
        let mut wake = Vec::new();
        let mut answered = false;
        for client in inner.clients.all.values_mut() {
            if !client.resync {
                if client.queue.len() + bytes.len() > OUTBOX_CAP {
                    client.queue = Vec::new();
                    client.resync = true;
                } else {
                    client.queue.extend_from_slice(bytes);
                }
            }
            // A responder that has lost these bytes will never see the query.
            answered |= client.responder && !client.resync;
            if !client.notified {
                client.notified = true;
                wake.push(Arc::clone(&client.notify));
            }
        }
        let replies = if answered {
            inner.owed = replies;
            Vec::new()
        } else {
            inner.owed.clear();
            replies
        };
        drop(inner);
        for notify in wake {
            notify();
        }
        replies
    }

    /// Adds a client whose first take is a snapshot, unless this is the very
    /// start of the session (`fresh`), when there is nothing to restore and
    /// the client answers ConPTY's opening query as it always has.
    /// At most one client answers queries, and the newest responder is it: an
    /// older one may be a page that is already gone, and its xterm is not
    /// answering, while the new one's xterm always does.
    fn add_client(&self, responder: bool, fresh: bool, notify: Notify) -> Attached {
        let mut inner = self.lock();
        if responder {
            for c in inner.clients.all.values_mut() {
                c.responder = false;
            }
        }
        let client = inner.clients.next;
        inner.clients.next += 1;
        inner.clients.all.insert(
            client,
            Client {
                queue: Vec::new(),
                notified: !fresh,
                resync: !fresh,
                responder,
                notify: Arc::clone(&notify),
            },
        );
        let exit = inner.exit;
        drop(inner);
        if !fresh {
            notify();
        }
        Attached { client, exit }
    }

    /// The client a new session starts with, before any output.
    pub fn first_client(&self, notify: Notify) -> ClientId {
        self.add_client(true, true, notify).client
    }

    pub fn attach(&self, responder: bool, notify: Notify) -> Attached {
        self.add_client(responder, false, notify)
    }

    /// The session keeps running. Queries sent to this client and not yet
    /// answered are lost (plan-19, section 3).
    pub fn detach(&self, client: ClientId) {
        self.lock().clients.all.remove(&client);
    }

    /// Every client went away at once: a page reload, or a lost connection.
    pub fn detach_all(&self) {
        self.lock().clients.all.clear();
    }

    /// Whether answers are owed that no client can still give: the responder is
    /// gone, or replaced by one that will only get a snapshot, which has no query
    /// in it.
    pub fn owes(&self) -> bool {
        let inner = self.lock();
        !inner.owed.is_empty() && !inner.clients.all.values().any(|c| c.responder && !c.resync)
    }

    /// The owed answers, for the caller to write to the child. Only after a wait:
    /// an answer the old page sent may still be on its way, and a second cursor
    /// report reads as F3, which PowerShell takes as a search for the next key.
    pub fn take_owed(&self) -> Vec<u8> {
        if !self.owes() {
            return Vec::new();
        }
        std::mem::take(&mut self.lock().owed)
    }

    /// Input from a client on its way to the child. A cursor report in it is the
    /// responder's answer, so nothing is owed.
    pub fn note_input(&self, data: &[u8]) {
        if has_cursor_report(data) {
            self.lock().owed.clear();
        }
    }

    /// Everything since the last take, or a snapshot if the client owes one.
    /// Clears `notified` under the lock `feed` sets it, so anything fed after
    /// this signals again.
    pub fn take(&self, client: ClientId) -> Taken {
        let mut inner = self.lock();
        let Some(resync) = inner.clients.all.get(&client).map(|c| c.resync) else {
            return Taken { bytes: Vec::new(), snapshot: false, finished: false };
        };
        let snapshot = if resync {
            Some(guarded(&mut inner.screen, |s| s.snapshot()).unwrap_or_else(|| RESET_ONLY.to_vec()))
        } else {
            None
        };
        let c = inner.clients.all.get_mut(&client).expect("checked above");
        c.notified = false;
        c.resync = false;
        let bytes = match snapshot {
            Some(s) => {
                c.queue = Vec::new();
                s
            }
            None => std::mem::take(&mut c.queue),
        };
        let snapshot = resync;
        let mut finished = false;
        if inner.eof && bytes.is_empty() {
            inner.clients.all.remove(&client);
            finished = inner.clients.all.is_empty();
        }
        Taken { bytes, snapshot, finished }
    }

    /// The client lost bytes on its side: drop its queue, and make its next
    /// take a snapshot (plan-19 D2).
    pub fn resync(&self, client: ClientId) {
        let mut inner = self.lock();
        let Some(c) = inner.clients.all.get_mut(&client) else { return };
        c.queue = Vec::new();
        c.resync = true;
        let wake = (!c.notified).then(|| {
            c.notified = true;
            Arc::clone(&c.notify)
        });
        drop(inner);
        if let Some(notify) = wake {
            notify();
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        guarded(&mut self.lock().screen, |s| s.resize(cols, rows));
    }

    /// The colours OSC 10/11 report while no responder is attached.
    pub fn set_theme(&self, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        let rgb = |(r, g, b): (u8, u8, u8)| Rgb { r, g, b };
        if let Some(s) = &self.lock().screen {
            s.heard.0.lock().unwrap_or_else(|p| p.into_inner()).theme = Some((rgb(fg), rgb(bg)));
        }
    }

    /// The child exited. With no client attached this is a tombstone: it stays
    /// until a client has seen the final screen and the code, or it is forgotten.
    pub fn set_exit(&self, code: Option<i32>) {
        self.lock().exit = Some(code);
    }

    pub fn exit(&self) -> Option<Option<i32>> {
        self.lock().exit
    }

    pub fn set_eof(&self) {
        self.lock().eof = true;
    }

    /// Nobody will take from this session again. The reader keeps reading to
    /// EOF, since ClosePseudoConsole waits for the pipe to drain, but it all goes
    /// nowhere.
    pub fn close(&self) {
        let mut inner = self.lock();
        inner.closed = true;
        inner.screen = None;
        inner.clients.all.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn quiet() -> Notify {
        Arc::new(|| {})
    }

    type Styled = (char, Color, Color, Flags);

    /// Every cell of the active grid, scrollback included.
    fn cells(s: &Screen) -> Vec<Styled> {
        let g = s.term.grid();
        let mut v = Vec::new();
        for li in -(g.history_size() as i32)..g.screen_lines() as i32 {
            for x in 0..g.columns() {
                let c = &g[Line(li)][Column(x)];
                v.push((c.c, c.fg, c.bg, c.flags));
            }
        }
        v
    }

    fn text(s: &Screen) -> String {
        cells(s).iter().map(|c| c.0).collect()
    }

    /// A fresh screen given `snapshot` then `rest` must match a control given
    /// the whole stream. Checked with `rest` too: a snapshot that looks right
    /// but leaves the wrong state only shows up in what follows.
    fn assert_restores(stream: &[u8], rest: &[u8]) {
        let mut original = Screen::new(40, 8);
        original.advance(stream);
        let snapshot = original.snapshot();

        let mut restored = Screen::new(40, 8);
        let replies = restored.advance(&snapshot);
        assert!(replies.is_empty(), "a snapshot must not make a terminal answer");
        assert_eq!(text(&restored), text(&original));
        assert_eq!(cells(&restored), cells(&original));
        assert_eq!(restored.term.mode(), original.term.mode());
        assert_eq!(restored.term.grid().cursor.point, original.term.grid().cursor.point);
        // Reaches what `cells` cannot see: the hidden main grid and the pen.
        assert_eq!(restored.snapshot(), snapshot, "the snapshot of a restore is the same snapshot");

        original.advance(rest);
        restored.advance(rest);
        assert_eq!(cells(&restored), cells(&original), "after continuing");
    }

    #[test]
    fn snapshot_restores_colours_and_attributes() {
        assert_restores(
            b"plain \x1b[1;31mbold red\x1b[0m \x1b[38;5;200;48;2;1;2;3mspec\x1b[0m \x1b[3;4:3;9mfancy\x1b[0m\r\nnext\x1b[32m",
            b" still green\r\n",
        );
    }

    #[test]
    fn snapshot_restores_wide_and_combining_glyphs() {
        assert_restores("日本語 e\u{301} ok 🙂 end\r\n".as_bytes(), "more 日本\r\n".as_bytes());
    }

    #[test]
    fn snapshot_restores_a_wrapped_line_and_a_pending_wrap() {
        let long = "x".repeat(95);
        assert_restores(long.as_bytes(), b"y");
        assert_restores("z".repeat(40).as_bytes(), b"w");
    }

    /// A full-screen app overwrites rows in place, so a row can keep its wrap
    /// flag after the row it wrapped into has been erased. Found by a long
    /// `claude` session: the blank row vanished and all below moved up one.
    #[test]
    fn snapshot_keeps_a_blank_row_under_a_stale_wrap() {
        let stream = format!("\x1b[?1049h{}y\x1b[2;1H\x1b[K\x1b[3;1Hbelow", "x".repeat(40));
        let mut original = Screen::new(40, 8);
        original.advance(stream.as_bytes());
        let g = original.term.grid();
        assert!(g[Line(0)][Column(39)].flags.contains(Flags::WRAPLINE), "the case this test is for");

        let mut restored = Screen::new(40, 8);
        restored.advance(&original.snapshot());
        // The stale flag itself does not come back, and should not.
        let unflagged = |s: &Screen| -> Vec<Styled> {
            cells(s).into_iter().map(|(c, fg, bg, f)| (c, fg, bg, f - Flags::WRAPLINE)).collect()
        };
        assert_eq!(unflagged(&restored), unflagged(&original));
        assert_eq!(restored.term.grid().cursor.point, original.term.grid().cursor.point);
    }

    #[test]
    fn snapshot_keeps_scrollback_up_to_the_cap_with_colours() {
        let mut stream = Vec::new();
        for i in 0..7000 {
            stream.extend_from_slice(format!("\x1b[3{}mline {i}\x1b[0m\r\n", i % 8).as_bytes());
        }
        let mut original = Screen::new(40, 8);
        original.advance(&stream);
        assert_eq!(original.term.grid().history_size(), SCROLLBACK);
        assert_restores(&stream, b"after\r\n");
    }

    #[test]
    fn snapshot_restores_modes_title_and_cursor() {
        assert_restores(
            b"\x1b]0;my title\x07\x1b[?1h\x1b=\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b[?25l\x1b[5 q\x1b[3;7Hhere",
            b"\x1b[?25hnow",
        );
        let mut s = Screen::new(40, 8);
        s.advance(b"\x1b]0;my title\x07");
        assert!(s.snapshot().windows(8).any(|w| w == b"my title"));
    }

    #[test]
    fn snapshot_restores_both_grids_from_the_alternate_screen() {
        assert_restores(
            b"main \x1b[31mcontent\x1b[0m\r\nline 2\x1b[?1049h\x1b[2J\x1b[Halt \x1b[32mscreen\x1b[0m",
            b" more\x1b[?1049lback on main\r\n",
        );
    }

    #[test]
    fn taking_a_snapshot_leaves_the_alternate_screen_as_it_was() {
        let mut s = Screen::new(40, 8);
        s.advance(b"main\x1b[?1049h\x1b[2J\x1b[Halt text");
        let before = cells(&s);
        let first = s.snapshot();
        assert_eq!(cells(&s), before);
        assert!(s.term.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(s.snapshot(), first);
    }

    #[test]
    fn snapshot_ends_with_a_partial_sequence_so_the_rest_is_not_printed() {
        assert_restores(b"before\x1b[3", b"1mred");
        assert_restores(b"before\x1b]0;half a ti", b"tle\x07after");
        assert_restores("before\u{65e5}".as_bytes().split_last().unwrap().1, &[0xa5, b'!']);
        let mut s = Screen::new(40, 8);
        s.advance(b"x\x1b[3");
        assert!(s.snapshot().ends_with(b"\x1b[3"));
    }

    #[test]
    fn snapshot_starts_with_a_reset_that_shows_the_cursor() {
        let mut s = Screen::new(40, 8);
        s.advance(b"\x1b[?25l");
        assert!(s.snapshot().starts_with(b"\x1bc\x1b[?25h"));
    }

    #[test]
    fn a_zero_size_is_survivable() {
        let mut s = Screen::new(0, 0);
        s.advance(b"hello\r\nworld");
        s.snapshot();
        s.resize(0, 0);
        s.advance(b"more");
        s.snapshot();
    }

    // ---- fan-out ----

    #[test]
    fn a_new_session_needs_no_snapshot_and_signals_once_until_taken() {
        let s = Session::new(40, 8);
        let signals = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&signals);
        let c = s.first_client(Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        s.feed(b"a");
        s.feed(b"b");
        assert_eq!(signals.load(Ordering::Relaxed), 1, "already announced");
        assert_eq!(s.take(c).bytes, b"ab");
        s.feed(b"c");
        assert_eq!(signals.load(Ordering::Relaxed), 2, "the take cleared it");
        assert_eq!(s.take(c).bytes, b"c");
        assert!(s.take(c).bytes.is_empty());
    }

    /// `resync` drops the queue, signals once, and the next take is a flagged
    /// snapshot; the take after that is the stream again (plan-19 D2, D8).
    #[test]
    fn resync_turns_the_next_take_into_a_snapshot() {
        let s = Session::new(40, 8);
        let signals = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&signals);
        let c = s.first_client(Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        s.feed(b"hello");
        let first = s.take(c);
        assert!(!first.snapshot);
        s.feed(b" world");
        s.resync(c);
        assert_eq!(signals.load(Ordering::Relaxed), 2, "feed signalled; resync did not need to");
        let snap = s.take(c);
        assert!(snap.snapshot);
        assert!(snap.bytes.starts_with(b"\x1bc"));
        s.resync(c);
        assert_eq!(signals.load(Ordering::Relaxed), 3, "nothing outstanding, so resync signals");
        assert!(s.take(c).snapshot);
        s.feed(b"!");
        let after = s.take(c);
        assert_eq!((after.bytes.as_slice(), after.snapshot), (&b"!"[..], false));
    }

    /// A reader feeds while a client attaches mid-stream: snapshot plus the
    /// bytes after it must be exactly the whole stream on screen.
    #[test]
    fn attach_during_output_has_no_gap_or_overlap() {
        let lines: Vec<Vec<u8>> = (0..4000).map(|i| format!("\x1b[3{}mline {i}\x1b[0m\r\n", i % 8).into_bytes()).collect();
        let session = Arc::new(Session::new(40, 8));
        let feeder = Arc::clone(&session);
        let stream = lines.clone();
        let reader = thread::spawn(move || {
            for chunk in &stream {
                // Cut through sequences as a real read can.
                let (a, b) = chunk.split_at(chunk.len() / 2);
                feeder.feed(a);
                feeder.feed(b);
            }
        });
        thread::sleep(std::time::Duration::from_millis(1));
        let client = session.attach(false, quiet()).client;
        let mut restored = Screen::new(40, 8);
        loop {
            let done = reader.is_finished();
            restored.advance(&session.take(client).bytes);
            if done {
                restored.advance(&session.take(client).bytes);
                break;
            }
        }
        reader.join().unwrap();

        let mut control = Screen::new(40, 8);
        for chunk in &lines {
            control.advance(chunk);
        }
        assert_eq!(cells(&restored), cells(&control));
    }

    #[test]
    fn overflow_drops_the_queue_and_the_next_take_is_a_snapshot() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        s.feed(b"first\r\n");
        for _ in 0..(OUTBOX_CAP / 1024 + 2) {
            s.feed(&[b'x'; 1024]);
        }
        s.feed(b"\r\nlast");
        let taken = s.take(c).bytes;
        assert!(taken.starts_with(b"\x1bc"), "a snapshot, not the queue");
        assert!(taken.len() < OUTBOX_CAP);
        let mut restored = Screen::new(40, 8);
        restored.advance(&taken);
        assert!(text(&restored).contains("last"));
        s.feed(b"after");
        assert_eq!(s.take(c).bytes, b"after", "back to the stream");
    }

    #[test]
    fn a_stalled_client_does_not_affect_another() {
        let s = Session::new(40, 8);
        let stalled = s.first_client(quiet());
        let live = s.attach(false, quiet()).client;
        s.take(live);
        let chunk = [b'x'; 1024];
        let mut seen = 0;
        for _ in 0..(OUTBOX_CAP / 1024 * 3) {
            s.feed(&chunk);
            let taken = s.take(live).bytes;
            assert_eq!(taken, chunk, "the live client sees the plain stream");
            seen += taken.len();
        }
        assert_eq!(seen, OUTBOX_CAP * 3, "no feed blocked on the stalled client");
        assert!(s.take(stalled).bytes.starts_with(b"\x1bc"));
    }

    #[test]
    fn with_no_client_a_flood_goes_nowhere_and_returns() {
        let s = Session::new(40, 8);
        for _ in 0..(OUTBOX_CAP / 1024 * 10) {
            s.feed(&[b'x'; 1024]);
        }
        s.feed(b"\r\nend");
        let c = s.attach(false, quiet()).client;
        let mut restored = Screen::new(40, 8);
        restored.advance(&s.take(c).bytes);
        assert!(text(&restored).contains("end"));
    }

    // ---- queries ----

    const DSR: &[u8] = b"\x1b[6n";

    #[test]
    fn a_responder_answers_so_the_session_does_not() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        assert!(s.feed(DSR).is_empty(), "one answer: the client's");
        assert_eq!(s.take(c).bytes, DSR, "the query reaches it");
    }

    #[test]
    fn with_no_responder_the_session_answers_once() {
        let s = Session::new(40, 8);
        assert_eq!(s.feed(b"\x1b[3;5H\x1b[6n"), b"\x1b[3;5R");
        assert!(s.feed(b"plain text").is_empty());
    }

    #[test]
    fn a_responder_gone_before_answering_leaves_the_answer_owed() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        assert!(s.feed(b"[3;5H[6n").is_empty());
        assert!(s.take_owed().is_empty(), "it can still answer");
        s.detach(c);
        assert_eq!(s.take_owed(), b"[3;5R", "or conhost waits for ever");
        assert!(s.take_owed().is_empty(), "once");
    }

    #[test]
    fn the_responders_own_answer_means_nothing_is_owed() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        assert!(s.feed(DSR).is_empty());
        s.note_input(b"x\x1b[A");
        s.detach(c);
        assert!(s.owes(), "keys are not an answer");
        s.note_input(b"\x1b[12;40R");
        assert!(!s.owes());
        assert!(s.take_owed().is_empty());
    }

    #[test]
    fn cursor_reports_are_found_and_lookalikes_are_not() {
        for yes in [&b"\x1b[1;1R"[..], b"ab\x1b[24;80Rcd", b"\x1b[\x1b[3;4R"] {
            assert!(has_cursor_report(yes), "{yes:?}");
        }
        for no in [&b"\x1b[A"[..], b"\x1b[;1R", b"\x1b[1;R", b"\x1b[1;2", b"1;1R", b"\x1b[1R"] {
            assert!(!has_cursor_report(no), "{no:?}");
        }
    }

    #[test]
    fn output_after_a_query_means_nothing_is_owed() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        assert!(s.feed(DSR).is_empty());
        assert!(s.feed(b"prompt> ").is_empty());
        s.detach(c);
        assert!(s.take_owed().is_empty());
    }

    #[test]
    fn a_new_responder_cannot_answer_what_its_snapshot_leaves_out() {
        let s = Session::new(40, 8);
        let first = s.first_client(quiet());
        assert!(s.feed(DSR).is_empty());
        let second = s.attach(true, quiet()).client;
        assert_eq!(s.take_owed(), b"[1;1R");
        s.take(first);
        s.take(second);
        assert!(s.feed(DSR).is_empty(), "caught up, it answers the next one");
        assert!(s.take_owed().is_empty());
    }

    #[test]
    fn a_non_responder_leaving_changes_nothing() {
        let s = Session::new(40, 8);
        let _responder = s.first_client(quiet());
        let watcher = s.attach(false, quiet()).client;
        assert!(s.feed(DSR).is_empty());
        s.detach(watcher);
        assert!(s.take_owed().is_empty());
    }

    #[test]
    fn a_non_responder_does_not_count() {
        let s = Session::new(40, 8);
        let watcher = s.attach(false, quiet()).client;
        s.take(watcher);
        assert_eq!(s.feed(DSR), b"\x1b[1;1R");
        assert_eq!(s.take(watcher).bytes, DSR, "it still sees the stream");
    }

    #[test]
    fn the_newest_responder_takes_the_role_so_a_query_is_answered_once() {
        let s = Session::new(40, 8);
        let old = s.attach(true, quiet()).client;
        s.take(old);
        let new = s.attach(true, quiet()).client;
        s.take(new);
        assert!(s.feed(DSR).is_empty(), "the new client answers");
        s.detach(old);
        assert!(s.feed(DSR).is_empty(), "a late detach of the old one changes nothing");
        s.detach(new);
        assert!(!s.feed(DSR).is_empty(), "nobody left: the session answers");
    }

    #[test]
    fn detach_all_leaves_the_shell_running_with_the_session_answering() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        s.detach_all();
        assert!(!s.feed(DSR).is_empty());
        assert!(s.take(c).bytes.is_empty(), "the old client is gone");
        let next = s.attach(true, quiet()).client;
        assert!(s.take(next).bytes.starts_with(b"\x1bc"), "and the next one gets a snapshot");
    }

    #[test]
    fn a_responder_owing_a_snapshot_cannot_answer_so_the_session_does() {
        let s = Session::new(40, 8);
        let c = s.attach(true, quiet()).client;
        assert_eq!(s.feed(DSR), b"\x1b[1;1R", "the snapshot will not carry the query");
        s.take(c);
        assert!(s.feed(DSR).is_empty(), "caught up: the responder has it");
    }

    #[test]
    fn a_responder_that_overflows_on_a_query_does_not_leave_it_unanswered() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        s.feed(&vec![b'x'; OUTBOX_CAP]);
        let reply = s.feed(DSR);
        assert!(reply.starts_with(b"\x1b[") && reply.ends_with(b"R"), "{reply:?}");
        assert!(s.take(c).bytes.starts_with(b"\x1bc"));
    }

    #[test]
    fn colour_queries_use_the_reported_theme_only_when_headless() {
        let s = Session::new(40, 8);
        assert!(s.feed(b"\x1b]10;?\x07").is_empty(), "nothing reported yet");
        s.set_theme((1, 2, 3), (255, 0, 16));
        let fg = String::from_utf8(s.feed(b"\x1b]10;?\x07")).unwrap();
        assert!(fg.starts_with("\x1b]10;rgb:0101/0202/0303"), "{fg:?}");
        let bg = String::from_utf8(s.feed(b"\x1b]11;?\x07")).unwrap();
        assert!(bg.starts_with("\x1b]11;rgb:ffff/0000/1010"), "{bg:?}");
        s.first_client(quiet());
        assert!(s.feed(b"\x1b]10;?\x07").is_empty(), "xterm answers");
    }

    // ---- ends ----

    #[test]
    fn a_tombstone_holds_the_screen_and_code_until_seen() {
        let s = Session::new(40, 8);
        s.feed(b"last words");
        s.set_exit(Some(3));
        s.set_eof();
        assert_eq!(s.exit(), Some(Some(3)));
        let attached = s.attach(true, quiet());
        assert_eq!(attached.exit, Some(Some(3)));
        let first = s.take(attached.client);
        assert!(!first.finished);
        let mut restored = Screen::new(40, 8);
        restored.advance(&first.bytes);
        assert!(text(&restored).contains("last words"));
        assert!(s.take(attached.client).finished, "seen: it can go");
    }

    #[test]
    fn eof_finishes_only_when_the_last_client_has_drained() {
        let s = Session::new(40, 8);
        let a = s.first_client(quiet());
        let b = s.attach(false, quiet()).client;
        s.feed(b"tail");
        s.set_eof();
        assert_eq!(s.take(a).bytes, b"tail");
        assert!(!s.take(a).finished, "b is still attached");
        assert!(!s.take(b).bytes.is_empty());
        assert!(s.take(b).finished);
    }

    #[test]
    fn close_discards_everything() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        s.close();
        assert!(s.feed(b"\x1b[6n").is_empty());
        assert!(s.take(c).bytes.is_empty());
    }

    #[test]
    fn an_emulator_panic_costs_snapshots_and_not_the_stream() {
        let s = Session::new(40, 8);
        let c = s.first_client(quiet());
        s.feed(b"before");
        s.lock().screen.as_mut().unwrap().explode = true;
        assert!(s.feed(b"boom").is_empty(), "feed survives");
        s.feed(b"after");
        assert_eq!(s.take(c).bytes, b"beforeboomafter", "the stream carries on");

        let late = s.attach(true, quiet()).client;
        assert_eq!(s.take(late).bytes, RESET_ONLY, "no snapshot left to offer");
        s.feed(b"more");
        assert_eq!(s.take(late).bytes, b"more");
    }
}
