//! A small file logger (plan-19). The daemon has no console, and plan-18 saw a
//! reader thread die on an `eprintln!` whose stderr had gone away, so nothing
//! in core writes to stderr. One file per process, rotated to `.1` at
//! `MAX_BYTES`, so at most two files exist. Logging never panics and never
//! fails a caller: before `init`, or after any I/O error, it does nothing.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_BYTES: u64 = 1024 * 1024;

struct Log {
    path: PathBuf,
    max: u64,
    file: Option<File>,
    len: u64,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

/// `%LOCALAPPDATA%\com.veal.app\logs`, or the temp folder without it.
pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("com.veal.app").join("logs")
}

/// Log to `<dir>/<name>.log` from now on. Only the first call counts.
pub fn init(name: &str) {
    init_at(dir().join(format!("{name}.log")), MAX_BYTES);
}

/// `init` with an explicit file and limit, for tests.
pub fn init_at(path: PathBuf, max: u64) {
    let _ = LOG.set(Mutex::new(open(path, max)));
}

fn open(path: PathBuf, max: u64) -> Log {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let len = file.as_ref().and_then(|f| f.metadata().ok()).map_or(0, |m| m.len());
    Log { path, max, file, len }
}

fn rotated(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".1");
    PathBuf::from(s)
}

impl Log {
    fn write(&mut self, line: &str) {
        if self.file.is_none() {
            return;
        }
        if self.len + line.len() as u64 > self.max && self.len > 0 {
            self.file = None;
            let old = rotated(&self.path);
            let _ = fs::remove_file(&old);
            let _ = fs::rename(&self.path, &old);
            *self = open(std::mem::take(&mut self.path), self.max);
        }
        let Some(f) = self.file.as_mut() else { return };
        if f.write_all(line.as_bytes()).is_ok() {
            self.len += line.len() as u64;
        } else {
            self.file = None;
        }
    }
}

/// One line: seconds since the epoch, thread name, message.
pub fn write(args: std::fmt::Arguments<'_>) {
    let Some(log) = LOG.get() else { return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let thread = std::thread::current();
    let line = format!(
        "{}.{:03} [{}] {}\n",
        now.as_secs(),
        now.subsec_millis(),
        thread.name().unwrap_or("?"),
        args
    );
    log.lock().unwrap_or_else(|p| p.into_inner()).write(&line);
}

/// `format!`-style logging to the process's log file.
#[macro_export]
macro_rules! vlog {
    ($($arg:tt)*) => {
        $crate::log::write(format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("veal-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn rotates_and_keeps_two_files() {
        let d = tmp("rotate");
        let path = d.join("t.log");
        let mut log = open(path.clone(), 100);
        for i in 0..20 {
            log.write(&format!("line {i:02} xxxxxxxxxx\n"));
        }
        let mut names: Vec<_> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, ["t.log", "t.log.1"]);
        assert!(fs::metadata(&path).unwrap().len() <= 100);
        let last = fs::read_to_string(&path).unwrap();
        assert!(last.ends_with("line 19 xxxxxxxxxx\n"), "{last}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unwritable_path_is_a_no_op() {
        // A file where the folder should be: create_dir_all and open both fail.
        let d = tmp("blocked");
        fs::create_dir_all(&d).unwrap();
        let blocker = d.join("dir");
        fs::write(&blocker, b"").unwrap();
        let mut log = open(blocker.join("t.log"), 100);
        log.write("nothing\n");
        assert!(log.file.is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_before_init_does_nothing() {
        // `LOG` may or may not be set by another test in this binary; either
        // way this must not panic.
        write(format_args!("hello {}", 1));
    }
}
