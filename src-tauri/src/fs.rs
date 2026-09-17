//! Read-only filesystem access for the Files view: list one directory, read
//! one file. Own commands rather than `tauri-plugin-fs` for the same reason
//! `db.rs` is not `tauri-plugin-sql` — two calls do not need a scoped plugin.
//!
//! Both commands are confined to pinned project roots (plan-04, decision 4).
//! The webview only ever runs our own code, so this is hardening, not a
//! security boundary.

use crate::db::Db;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Files above this are reported by size, not read (`too large`).
pub const READ_CAP: u64 = 2 * 1024 * 1024;
/// A NUL byte within this many leading bytes marks a file as binary.
const SNIFF_LEN: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    pub kind: &'static str,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileText {
    pub text: String,
    pub size: u64,
}

/// Directories first, then case-insensitive by name; `.git` is hidden.
pub fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<Entry> = read
        .filter_map(|r| r.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == ".git" {
                return None;
            }
            // `metadata` follows symlinks, so a link to a directory lists as
            // one; a broken link falls through as a zero-byte file.
            let (kind, size) = match std::fs::metadata(e.path()) {
                Ok(m) if m.is_dir() => ("dir", 0),
                Ok(m) => ("file", m.len()),
                Err(_) => ("file", 0),
            };
            Some(Entry { name, kind, size })
        })
        .collect();
    entries.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

/// UTF-8 (lossily decoded, BOM stripped). Errors: `too large: N bytes`,
/// `binary`, or the io error.
pub fn read(path: &Path) -> Result<FileText, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if size > READ_CAP {
        return Err(format!("too large: {size} bytes"));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    std::fs::File::open(path)
        .and_then(|mut f| f.read_to_end(&mut bytes))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes[..bytes.len().min(SNIFF_LEN)].contains(&0) {
        return Err("binary".into());
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.starts_with('\u{feff}') {
        text.remove(0);
    }
    Ok(FileText { text, size })
}

/// The canonical form of `path` if it lies under a pinned project root.
pub fn confine(db: &Db, path: &str) -> Result<PathBuf, String> {
    let outside = || format!("outside pinned projects: {path}");
    let canon = std::fs::canonicalize(path.trim()).map_err(|e| format!("{path}: {e}"))?;
    let inside = db.projects_list()?.iter().any(|p| {
        std::fs::canonicalize(&p.path)
            .map(|root| canon.starts_with(root))
            .unwrap_or(false)
    });
    if inside {
        Ok(canon)
    } else {
        Err(outside())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn list_sorts_dirs_first_case_insensitive_and_hides_git() {
        let tmp = tempfile::tempdir().unwrap();
        for d in ["b", "A", ".git"] {
            fs::create_dir(tmp.path().join(d)).unwrap();
        }
        fs::write(tmp.path().join("z.txt"), "z").unwrap();
        fs::write(tmp.path().join("B.txt"), "bb").unwrap();
        let entries = list(tmp.path()).unwrap();
        assert_eq!(names(&entries), ["A", "b", "B.txt", "z.txt"]);
        assert_eq!(entries[0].kind, "dir");
        assert_eq!(entries[2], Entry { name: "B.txt".into(), kind: "file", size: 2 });
    }

    #[test]
    fn list_missing_dir_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list(&tmp.path().join("nope")).is_err());
    }

    #[test]
    fn read_strips_bom_and_keeps_crlf() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        fs::write(&p, b"\xEF\xBB\xBFone\r\ntwo\n").unwrap();
        let f = read(&p).unwrap();
        assert_eq!(f.text, "one\r\ntwo\n");
        assert_eq!(f.size, 12);
    }

    #[test]
    fn read_rejects_binary_and_too_large() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("a.bin");
        fs::write(&bin, [0u8, 1, 2]).unwrap();
        assert_eq!(read(&bin).unwrap_err(), "binary");

        let big = tmp.path().join("big.txt");
        fs::write(&big, vec![b'x'; READ_CAP as usize + 1]).unwrap();
        assert!(read(&big).unwrap_err().starts_with("too large: "));
    }

    #[test]
    fn confine_requires_a_pinned_root() {
        let inside = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(inside.path().join("f.txt"), "x").unwrap();
        fs::write(outside.path().join("f.txt"), "x").unwrap();

        let db = Db::open_in_memory().unwrap();
        db.projects_add("in", inside.path().to_str().unwrap()).unwrap();

        assert!(confine(&db, inside.path().join("f.txt").to_str().unwrap()).is_ok());
        let err = confine(&db, outside.path().join("f.txt").to_str().unwrap()).unwrap_err();
        assert!(err.starts_with("outside pinned projects: "));
    }
}
