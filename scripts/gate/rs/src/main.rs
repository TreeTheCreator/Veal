// Gate harness for plan-19 section 0.
//   gate pty <cols> <rows> <cwd> <program> [args]   stdin: W<b64> | R<c> <r> ; stdout: D<b64> | X<code>
//   gate snap <candidate> <events.bin> <cuts.txt> <outdir>
//   gate bench <events.bin> <repeat>
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use base64::Engine;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};
use veal_core::session::{Session, SCROLLBACK};

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

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

// ---- candidates ----

trait Candidate {
    fn feed(&mut self, b: &[u8]);
    fn resize(&mut self, c: u16, r: u16);
    fn snapshot(&mut self) -> Vec<u8>;
}

/// What ships: `Session` from veal-core. A snapshot is what a client's first take
/// gives it, so this exercises the real attach path as well as the serializer.
struct Shipped(Session);
impl Candidate for Shipped {
    fn feed(&mut self, b: &[u8]) {
        self.0.feed(b);
    }
    fn resize(&mut self, c: u16, r: u16) {
        self.0.resize(c, r);
    }
    fn snapshot(&mut self) -> Vec<u8> {
        let client = self.0.attach(false, Arc::new(|| {})).client;
        let bytes = self.0.take(client).bytes;
        self.0.detach(client);
        bytes
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
        "alacritty-tail" => Box::new(Shipped(Session::new(c, r))),
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
    let mut c = Shipped(Session::new(cols, rows));
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
    let mbps = (total * repeat) as f64 / 1e6 / secs;
    println!(
        "{{\"cols\":{cols},\"rows\":{rows},\"mb\":{:.1},\"secs\":{secs:.3},\"mbPerSec\":{mbps:.1},\"corePctAt195KBs\":{:.2},\"rssDeltaMB\":{:.1},\"snapshotMs\":{snap_ms:.1},\"snapshotKB\":{}}}",
        (total * repeat) as f64 / 1e6,
        0.195 / mbps * 100.0,
        (after.saturating_sub(before)) as f64 / 1e6,
        snap.len() / 1024
    );
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

// The emulator's own view of its grid, with no serializer involved: logical lines, one per row of output.
fn dump_mode(a: &[String]) {
    let (cols, rows, evs) = read_events(&a[0]);
    let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
    let mut term = Term::new(config, &Size(cols as usize, rows as usize), VoidListener);
    let mut parser: Processor = Processor::new();
    for e in &evs {
        match e {
            Ev::Data(d) => parser.advance(&mut term, d),
            Ev::Resize(cc, r) => term.resize(Size(*cc as usize, *r as usize)),
        }
    }
    let g = term.grid();
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
