// Gate harness for plan-19 section 0. Throwaway.
//   gate pty <cols> <rows> <cwd> <program> [args]   stdin: W<b64> | R<c> <r> ; stdout: D<b64> | X<code>
//   gate snap <candidate> <events.bin> <cuts.txt> <outdir>
//   gate bench <events.bin> <repeat>
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor};
use base64::Engine;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const SCROLLBACK: usize = 5000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args[0].as_str() {
        "pty" => pty_mode(&args[1..]),
        "snap" => snap_mode(&args[1..]),
        "bench" => bench_mode(&args[1..]),
        "dump" => dump_mode(&args[1..]),
        m => panic!("unknown mode {m}"),
    }
}

// ---- recording: a real ConPTY driven from Node ----

fn pty_mode(a: &[String]) {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    let (cols, rows): (u16, u16) = (a[0].parse().unwrap(), a[1].parse().unwrap());
    let pair = native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).unwrap();
    let mut cmd = CommandBuilder::new(&a[3]);
    cmd.args(&a[4..]);
    cmd.cwd(&a[2]);
    cmd.env("TERM", "xterm-256color");
    cmd.env("VEAL_PANE", "1");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let o2 = out.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 16 * 1024]; // READ_BUF in pty.rs
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut o = o2.lock().unwrap();
                    writeln!(o, "D{}", B64.encode(&buf[..n])).unwrap();
                    o.flush().unwrap();
                }
            }
        }
    });
    let o3 = out.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|s| s.exit_code() as i32).unwrap_or(-1);
        let mut o = o3.lock().unwrap();
        writeln!(o, "X{code}").unwrap();
        o.flush().unwrap();
        std::process::exit(0);
    });
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        match line.as_bytes().first() {
            Some(b'W') => {
                writer.write_all(&B64.decode(&line[1..]).unwrap()).unwrap();
                writer.flush().unwrap();
            }
            Some(b'R') => {
                let mut p = line[1..].split(' ');
                let cols = p.next().unwrap().parse().unwrap();
                let rows = p.next().unwrap().parse().unwrap();
                pair.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).unwrap();
            }
            _ => {}
        }
    }
}

// ---- recorded events: [u8 kind][u32 le len][payload]; D bytes, R u16 cols u16 rows ----

enum Ev {
    Data(Vec<u8>),
    Resize(u16, u16),
}

fn read_events(path: &str) -> (u16, u16, Vec<Ev>) {
    let b = std::fs::read(path).unwrap();
    let cols = u16::from_le_bytes([b[0], b[1]]);
    let rows = u16::from_le_bytes([b[2], b[3]]);
    let mut i = 4;
    let mut v = Vec::new();
    while i < b.len() {
        let k = b[i];
        let n = u32::from_le_bytes([b[i + 1], b[i + 2], b[i + 3], b[i + 4]]) as usize;
        let p = &b[i + 5..i + 5 + n];
        i += 5 + n;
        v.push(if k == b'D' {
            Ev::Data(p.to_vec())
        } else {
            Ev::Resize(u16::from_le_bytes([p[0], p[1]]), u16::from_le_bytes([p[2], p[3]]))
        });
    }
    (cols, rows, v)
}

// ---- the partial-sequence tracker: bytes the parser is holding at a cut ----

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
    fn byte(&mut self, c: u8) {
        match self.state {
            0 => {
                if c == 0x1b {
                    self.bytes.clear();
                    self.bytes.push(c);
                    self.state = 1;
                } else if c >= 0xc2 && c < 0xf5 {
                    self.bytes.clear();
                    self.bytes.push(c);
                    self.need = if c < 0xe0 { 1 } else if c < 0xf0 { 2 } else { 3 };
                    self.state = 5;
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
                    self.bytes.clear();
                    self.bytes.push(c);
                    self.state = 1;
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
            4 => {
                self.bytes.push(c);
                if c == b'\\' {
                    self.done();
                } else {
                    self.state = 3;
                }
            }
            _ => unreachable!(),
        }
    }
    fn done(&mut self) {
        self.state = 0;
        self.bytes.clear();
    }
}

// ---- candidates ----

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Shared>>);
#[derive(Default)]
struct Shared {
    title: Option<String>,
    replies: usize,
}
impl EventListener for Listener {
    fn send_event(&self, e: Event) {
        let mut s = self.0.lock().unwrap();
        match e {
            Event::Title(t) => s.title = Some(t),
            Event::ResetTitle => s.title = None,
            Event::PtyWrite(_) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(_) => s.replies += 1,
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

trait Candidate {
    fn feed(&mut self, b: &[u8]);
    fn resize(&mut self, c: u16, r: u16);
    fn snapshot(&mut self) -> Vec<u8>;
}

struct Alac {
    term: Term<Listener>,
    proc: Processor,
    l: Listener,
    tail: Tail,
    use_tail: bool,
}
impl Alac {
    fn new(c: u16, r: u16, use_tail: bool) -> Self {
        let l = Listener::default();
        let cfg = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        Alac { term: Term::new(cfg, &Size(c as usize, r as usize), l.clone()), proc: Processor::new(), l, tail: Tail::default(), use_tail }
    }
}

fn color(out: &mut String, c: Color, base: u32) {
    use std::fmt::Write;
    match c {
        Color::Named(NamedColor::Foreground) | Color::Named(NamedColor::Background) | Color::Named(NamedColor::Cursor) => {}
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
    if f.contains(Flags::BOLD) { s.push_str(";1"); }
    if f.contains(Flags::DIM) { s.push_str(";2"); }
    if f.contains(Flags::ITALIC) { s.push_str(";3"); }
    if f.contains(Flags::UNDERCURL) { s.push_str(";4:3"); }
    else if f.contains(Flags::DOTTED_UNDERLINE) { s.push_str(";4:4"); }
    else if f.contains(Flags::DASHED_UNDERLINE) { s.push_str(";4:5"); }
    else if f.contains(Flags::DOUBLE_UNDERLINE) { s.push_str(";4:2"); }
    else if f.contains(Flags::UNDERLINE) { s.push_str(";4"); }
    if f.contains(Flags::INVERSE) { s.push_str(";7"); }
    if f.contains(Flags::HIDDEN) { s.push_str(";8"); }
    if f.contains(Flags::STRIKEOUT) { s.push_str(";9"); }
    color(&mut s, c.fg, 30);
    color(&mut s, c.bg, 40);
    let uc = c.underline_color();
    if let Some(u) = uc {
        match u {
            Color::Indexed(i) => write!(s, ";58;5;{i}").unwrap(),
            Color::Spec(r) => write!(s, ";58;2;{};{};{}", r.r, r.g, r.b).unwrap(),
            _ => {}
        }
    }
    s.push('m');
    let visible = f & !(Flags::WRAPLINE | Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    ((c.fg, c.bg, visible.bits(), uc), s)
}

fn is_blank(c: &Cell) -> bool {
    let visible = c.flags & !(Flags::WRAPLINE);
    c.c == ' ' && visible.is_empty() && c.bg == Color::Named(NamedColor::Background) && c.zerowidth().is_none() && c.hyperlink().is_none()
}

/// Every line of a grid from `first` to the bottom, as bytes that redraw it.
fn write_grid(out: &mut Vec<u8>, g: &Grid<Cell>, first: i32) {
    let cols = g.columns();
    let last_line = g.screen_lines() as i32 - 1;
    let mut pen: Option<PenKey> = None;
    let mut link: Option<String> = None;
    let mut buf = [0u8; 4];
    for li in first..=last_line {
        let row = &g[Line(li)];
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        let end = if wrapped {
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
            if let Some(z) = cell.zerowidth() {
                for &ch in z {
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
            // Width tables can disagree between the two emulators; a CHA after a
            // non-ASCII glyph stops one disagreement shifting the rest of the row.
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
    let (line, col) = (p.line.0 + 1, p.column.0 + 1);
    out.extend_from_slice(format!("\x1b[{line};{col}H").as_bytes());
    // Pending wrap: the cursor sits past the last column until the next glyph.
    // Only re-printing the last cell recreates that state.
    if g.cursor.input_needs_wrap {
        let c = &g[p.line][p.column];
        let (_, code) = sgr(c);
        out.extend_from_slice(code.as_bytes());
        let mut b = [0u8; 4];
        out.extend_from_slice(c.c.encode_utf8(&mut b).as_bytes());
    }
}

impl Candidate for Alac {
    fn feed(&mut self, b: &[u8]) {
        self.proc.advance(&mut self.term, b);
        if self.use_tail {
            self.tail.feed(b);
        }
    }
    fn resize(&mut self, c: u16, r: u16) {
        self.term.resize(Size(c as usize, r as usize));
    }
    fn snapshot(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 * 1024);
        out.extend_from_slice(b"\x1bc\x1b[?25h");
        let mode = *self.term.mode();
        let alt = mode.contains(TermMode::ALT_SCREEN);
        // The hidden main grid is private. Swapping is the only way to reach it;
        // swapping back wipes the alt grid, so a clone puts it back afterwards.
        let saved_alt = if alt { Some(self.term.grid().clone()) } else { None };
        if alt {
            self.term.swap_alt();
        }
        {
            let g = self.term.grid();
            write_grid(&mut out, g, -(g.history_size() as i32));
            let s = g.saved_cursor.point;
            out.extend_from_slice(format!("\x1b[{};{}H\x1b7", s.line.0 + 1, s.column.0 + 1).as_bytes());
            place_cursor(&mut out, g);
        }
        if let Some(a) = saved_alt {
            let main_saved = self.term.grid().saved_cursor.clone();
            self.term.swap_alt();
            let (inactive_cursor, _) = (a.cursor.clone(), ());
            *self.term.grid_mut() = a;
            self.term.grid_mut().cursor = inactive_cursor;
            let _ = main_saved;
            out.extend_from_slice(b"\x1b[?1049h\x1b[H");
            write_grid(&mut out, self.term.grid(), 0);
            place_cursor(&mut out, self.term.grid());
        }
        // Current pen.
        let tmpl = self.term.grid().cursor.template.clone();
        out.extend_from_slice(sgr(&tmpl).1.as_bytes());
        // Modes, last: wrap must be on while the rows above are drawn.
        let mut m = |flag: TermMode, seq: &str| {
            if mode.contains(flag) {
                out.extend_from_slice(seq.as_bytes());
            }
        };
        m(TermMode::APP_CURSOR, "\x1b[?1h");
        m(TermMode::APP_KEYPAD, "\x1b=");
        m(TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h");
        m(TermMode::MOUSE_DRAG, "\x1b[?1002h");
        m(TermMode::MOUSE_MOTION, "\x1b[?1003h");
        m(TermMode::UTF8_MOUSE, "\x1b[?1005h");
        m(TermMode::SGR_MOUSE, "\x1b[?1006h");
        m(TermMode::BRACKETED_PASTE, "\x1b[?2004h");
        m(TermMode::FOCUS_IN_OUT, "\x1b[?1004h");
        m(TermMode::INSERT, "\x1b[4h");
        m(TermMode::LINE_FEED_NEW_LINE, "\x1b[20h");
        if !mode.contains(TermMode::LINE_WRAP) {
            out.extend_from_slice(b"\x1b[?7l");
        }
        if !mode.contains(TermMode::SHOW_CURSOR) {
            out.extend_from_slice(b"\x1b[?25l");
        }
        let cs = self.term.cursor_style();
        let code = match (cs.shape, cs.blinking) {
            (CursorShape::Block, true) => Some(1),
            (CursorShape::Block, false) => None,
            (CursorShape::Underline, true) => Some(3),
            (CursorShape::Underline, false) => Some(4),
            (CursorShape::Beam, true) => Some(5),
            (CursorShape::Beam, false) => Some(6),
            _ => None,
        };
        if let Some(c) = code {
            out.extend_from_slice(format!("\x1b[{c} q").as_bytes());
        }
        if let Some(t) = self.l.0.lock().unwrap().title.clone() {
            out.extend_from_slice(format!("\x1b]0;{t}\x07").as_bytes());
        }
        if self.use_tail {
            out.extend_from_slice(&self.tail.bytes);
        }
        out
    }
}

struct Vt {
    p: vt100::Parser,
}
impl Candidate for Vt {
    fn feed(&mut self, b: &[u8]) {
        self.p.process(b);
    }
    fn resize(&mut self, c: u16, r: u16) {
        self.p.screen_mut().set_size(std::num::NonZeroU16::new(r).unwrap(), std::num::NonZeroU16::new(c).unwrap());
    }
    fn snapshot(&mut self) -> Vec<u8> {
        let (rows, cols) = self.p.screen().size();
        let (rows_n, cols_n) = (rows.get(), cols.get());
        let mut out = b"\x1bc".to_vec();
        // Scrollback: page through it with set_scrollback; the crate has no other view.
        self.p.screen_mut().set_scrollback(usize::MAX);
        let total = self.p.screen().scrollback();
        let mut left = total;
        while left > 0 {
            self.p.screen_mut().set_scrollback(left);
            let n = left.min(rows_n as usize);
            let lines: Vec<String> = self.p.screen().rows_formatted(0, cols_n).take(n).collect();
            for l in lines {
                out.extend_from_slice(l.as_bytes());
                out.extend_from_slice(b"\x1b[0m\r\n");
            }
            left -= n;
        }
        self.p.screen_mut().set_scrollback(0);
        // contents_formatted starts with a clear, so make room first: scrolling the
        // scrollback rows out needs `rows` newlines.
        for _ in 0..rows_n {
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(self.p.screen().state_formatted().as_bytes());
        out.extend_from_slice(self.p.screen().input_mode_formatted().as_bytes());
        out
    }
}

fn candidate(name: &str, c: u16, r: u16) -> Box<dyn Candidate> {
    match name {
        "alacritty" => Box::new(Alac::new(c, r, false)),
        "alacritty-tail" => Box::new(Alac::new(c, r, true)),
        "vt100" => Box::new(Vt { p: vt100::Parser::new(std::num::NonZeroU16::new(r).unwrap(), std::num::NonZeroU16::new(c).unwrap(), SCROLLBACK) }),
        n => panic!("unknown candidate {n}"),
    }
}

// Cut file: one "<event index> <byte offset>" per line, in stream order.
fn snap_mode(a: &[String]) {
    let (cols, rows, evs) = read_events(&a[1]);
    let cuts: Vec<(usize, usize)> = std::fs::read_to_string(&a[2])
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut p = l.split(' ');
            (p.next().unwrap().parse().unwrap(), p.next().unwrap().parse().unwrap())
        })
        .collect();
    std::fs::create_dir_all(&a[3]).unwrap();
    let mut c = candidate(&a[0], cols, rows);
    let mut next = 0;
    let mut times = Vec::new();
    let mut snap = |c: &mut Box<dyn Candidate>, next: &mut usize| {
        let t = std::time::Instant::now();
        let s = c.snapshot();
        times.push((t.elapsed().as_secs_f64() * 1000.0, s.len()));
        std::fs::write(format!("{}/{}.snap", a[3], *next), s).unwrap();
        *next += 1;
    };
    for (i, e) in evs.iter().enumerate() {
        match e {
            Ev::Resize(cc, r) => {
                while next < cuts.len() && cuts[next].0 == i {
                    snap(&mut c, &mut next);
                }
                c.resize(*cc, *r)
            }
            Ev::Data(d) => {
                let mut pos = 0;
                while next < cuts.len() && cuts[next].0 == i {
                    let at = cuts[next].1.min(d.len());
                    c.feed(&d[pos..at]);
                    pos = at;
                    snap(&mut c, &mut next);
                }
                c.feed(&d[pos..]);
            }
        }
    }
    while next < cuts.len() {
        snap(&mut c, &mut next);
    }
    let max = times.iter().map(|t| t.0).fold(0.0, f64::max);
    let maxb = times.iter().map(|t| t.1).max().unwrap_or(0);
    println!("{{\"snapshots\":{},\"maxMs\":{max:.2},\"maxBytes\":{maxb}}}", times.len());
}

fn rss() -> u64 {
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().unwrap();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

fn bench_mode(a: &[String]) {
    let (cols, rows, evs) = read_events(&a[0]);
    let repeat: usize = a[1].parse().unwrap();
    let data: Vec<&[u8]> = evs.iter().filter_map(|e| if let Ev::Data(d) = e { Some(d.as_slice()) } else { None }).collect();
    let total: usize = data.iter().map(|d| d.len()).sum();
    let before = rss();
    let mut c = Alac::new(cols, rows, true);
    let t = std::time::Instant::now();
    for _ in 0..repeat {
        for d in &data {
            c.feed(d);
        }
    }
    let secs = t.elapsed().as_secs_f64();
    let after = rss();
    let t2 = std::time::Instant::now();
    let snap = c.snapshot();
    let snap_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let hist = c.term.grid().history_size();
    let mbps = (total * repeat) as f64 / 1e6 / secs;
    println!(
        "{{\"cols\":{cols},\"rows\":{rows},\"mb\":{:.1},\"secs\":{secs:.3},\"mbPerSec\":{mbps:.1},\"corePctAt195KBs\":{:.2},\"history\":{hist},\"rssDeltaMB\":{:.1},\"snapshotMs\":{snap_ms:.1},\"snapshotKB\":{}}}",
        (total * repeat) as f64 / 1e6,
        0.195 / mbps * 100.0,
        (after.saturating_sub(before)) as f64 / 1e6,
        snap.len() / 1024
    );
}


// The emulator's own view of its grid, with no serializer involved: logical lines, one per row of output.
fn dump_mode(a: &[String]) {
    let (cols, rows, evs) = read_events(&a[0]);
    let mut c = Alac::new(cols, rows, false);
    for e in &evs {
        match e {
            Ev::Data(d) => c.feed(d),
            Ev::Resize(cc, r) => c.resize(*cc, *r),
        }
    }
    let g = c.term.grid();
    let n = g.columns();
    let mut cur = String::new();
    for li in -(g.history_size() as i32)..g.screen_lines() as i32 {
        let row = &g[Line(li)];
        let mut text = String::new();
        for x in 0..n {
            let cell = &row[Column(x)];
            if !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                text.push(cell.c);
            }
        }
        if row[Column(n - 1)].flags.contains(Flags::WRAPLINE) {
            cur.push_str(&text);
        } else {
            cur.push_str(text.trim_end());
            println!("{cur}");
            cur.clear();
        }
    }
}
