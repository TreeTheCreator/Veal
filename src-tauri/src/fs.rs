//! Filesystem access for the Files view: list one directory, read one file,
//! write one file back (plan-06), create / rename / delete an entry
//! (plan-09). Own commands rather than `tauri-plugin-fs` for the same
//! reason `db.rs` is not `tauri-plugin-sql` — a handful of calls do not
//! need a scoped plugin.
//!
//! Every command is confined to pinned project roots (plan-04, decision 4).
//! The webview only ever runs our own code, so this is hardening, not a
//! security boundary.

use crate::db::Db;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

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
    /// Modification time in whole milliseconds since the epoch; the save's
    /// conflict check compares against it.
    pub mtime: u64,
    /// True when a UTF-8 BOM was stripped from `text`; `write` restores it.
    pub bom: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Written {
    pub size: u64,
    pub mtime: u64,
}

const BOM: &str = "\u{feff}";
const TMP_SUFFIX: &str = ".veal-tmp";

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let size = meta.len();
    let mtime = mtime_ms(&meta);
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
    let bom = text.starts_with(BOM);
    if bom {
        text.drain(..BOM.len());
    }
    Ok(FileText { text, size, mtime, bom })
}

/// Hidden / system / archive etc. on Windows (plan-13): the rename below
/// makes a new file, so what the old one had is put back afterwards.
#[cfg(windows)]
fn file_attributes(meta: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(windows)]
fn restore_attributes(path: &Path, attrs: u32) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_NORMAL};
    if attrs == 0 || attrs == FILE_ATTRIBUTE_NORMAL {
        return;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // Best effort: the content is saved either way, and the mtime was read after the rename.
    unsafe {
        SetFileAttributesW(wide.as_ptr(), attrs);
    }
}

/// Writes `text` (LF-joined, as CodeMirror hands it over) back to `path`,
/// re-applying CRLF and the BOM the file had, atomically via a sibling temp
/// file. `expected_mtime: Some(m)` refuses the write unless the file's
/// current mtime is still `m` — the "never clobber an agent's edit" rule.
/// A read-only file is refused up front (`read-only on disk`), and on
/// Windows the file's attributes survive the rename.
pub fn write(
    path: &Path,
    text: &str,
    crlf: bool,
    bom: bool,
    expected_mtime: Option<u64>,
) -> Result<Written, String> {
    let before = match std::fs::metadata(path) {
        Ok(meta) => Some(meta),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if let Some(expected) = expected_mtime {
        match &before {
            Some(meta) if mtime_ms(meta) == expected => {}
            Some(_) => return Err("changed on disk".into()),
            None => return Err("deleted on disk".into()),
        }
    }
    if before.as_ref().is_some_and(|m| m.permissions().readonly()) {
        return Err("read-only on disk".into());
    }
    #[cfg(windows)]
    let attrs = before.as_ref().map(file_attributes);
    let mut out = String::with_capacity(text.len() + BOM.len() + text.len() / 16);
    if bom {
        out.push_str(BOM);
    }
    if crlf {
        out.push_str(&text.replace('\n', "\r\n"));
    } else {
        out.push_str(text);
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(TMP_SUFFIX);
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, out.as_bytes()).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("rename {}: {e}", path.display()));
    }
    #[cfg(windows)]
    if let Some(attrs) = attrs {
        restore_attributes(path, attrs);
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Written {
        size: meta.len(),
        mtime: mtime_ms(&meta),
    })
}

// ---- create / rename / delete (plan-09) ----

const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
    "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Windows-strict on every platform (decision 3): a name a teammate on
/// Windows cannot check out is not a name. Mirrored in src/files/names.ts.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("name is empty".into());
    }
    if name != name.trim() {
        return Err("no leading or trailing spaces".into());
    }
    if name == "." || name == ".." {
        return Err("not a valid name".into());
    }
    if name.contains(['\\', '/']) {
        return Err("no slashes in a name".into());
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control()) {
        return Err(if c.is_control() { "no control characters".into() } else { format!("{c} is not allowed in a name") });
    }
    if name.ends_with('.') {
        return Err("a name cannot end with a dot".into());
    }
    if name.len() > 255 {
        return Err("name is too long".into());
    }
    let stem = name.split('.').next().unwrap_or("").to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("{} is a reserved name on Windows", stem.to_uppercase()));
    }
    Ok(())
}

/// Creates an empty file or a directory named `name` in `dir`; refuses an
/// existing entry. Returns the new path.
pub fn create(dir: &Path, name: &str, kind: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let path = dir.join(name);
    let exists = || format!("already exists: {name}");
    match kind {
        "dir" => {
            if path.exists() {
                return Err(exists());
            }
            std::fs::create_dir(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        }
        "file" => {
            std::fs::File::create_new(&path).map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => exists(),
                _ => format!("create {}: {e}", path.display()),
            })?;
        }
        other => return Err(format!("unknown kind: {other}")),
    }
    Ok(path)
}

/// Renames `path` to a sibling called `name`. An existing sibling is
/// refused — unless it is this very entry under a different case
/// (decision 7), which a case-insensitive filesystem renames in place.
pub fn rename(path: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let parent = path.parent().ok_or_else(|| format!("no parent: {}", path.display()))?;
    let target = parent.join(name);
    let same_entry = path
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase() == name.to_lowercase())
        .unwrap_or(false);
    if target.exists() && !same_entry {
        return Err(format!("already exists: {name}"));
    }
    std::fs::rename(path, &target).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(target)
}

/// Moves `path` to the Recycle Bin (decision 1). Not unit-tested — it would
/// fill the developer's bin on every `cargo test`; acceptance covers it.
pub fn delete(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("not found".into());
    }
    trash::delete(path).map_err(|e| format!("delete {}: {e}", path.display()))
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
    fn write_refuses_a_read_only_file_and_leaves_no_temp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("locked.txt");
        fs::write(&path, "keep").unwrap();
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&path, perm).unwrap();
        assert_eq!(write(&path, "new", false, false, None), Err("read-only on disk".into()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep");
        assert!(!path.with_extension("txt.veal-tmp").exists());
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_readonly(false);
        fs::set_permissions(&path, perm).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn write_keeps_the_hidden_attribute() {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secret.txt");
        fs::write(&path, "old").unwrap();
        restore_attributes(&path, fs::metadata(&path).unwrap().file_attributes() | FILE_ATTRIBUTE_HIDDEN);
        assert_ne!(fs::metadata(&path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN, 0, "setup");
        write(&path, "new", false, false, None).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_ne!(fs::metadata(&path).unwrap().file_attributes() & FILE_ATTRIBUTE_HIDDEN, 0, "hidden bit survives a save");
    }

    #[test]
    fn validate_name_is_windows_strict() {
        for ok in ["a.txt", ".gitignore", "émoji 🐟.md", "console.log", "connect.rs", "nulls.md"] {
            assert_eq!(validate_name(ok), Ok(()), "{ok}");
        }
        let bad = [
            ("", "name is empty"),
            (" ", "name is empty"),
            (" a", "no leading or trailing spaces"),
            ("a ", "no leading or trailing spaces"),
            (".", "not a valid name"),
            ("..", "not a valid name"),
            ("a/b", "no slashes in a name"),
            ("a\\b", "no slashes in a name"),
            ("a:b", ": is not allowed in a name"),
            ("a?b", "? is not allowed in a name"),
            ("a\tb", "no control characters"),
            ("con", "CON is a reserved name on Windows"),
            ("CON.txt", "CON is a reserved name on Windows"),
            ("lpt1", "LPT1 is a reserved name on Windows"),
            ("a.", "a name cannot end with a dot"),
        ];
        for (name, err) in bad {
            assert_eq!(validate_name(name), Err(err.into()), "{name:?}");
        }
        assert_eq!(validate_name(&"x".repeat(256)), Err("name is too long".into()));
        assert_eq!(validate_name(&"x".repeat(255)), Ok(()));
    }

    #[test]
    fn create_file_and_dir_refuse_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let f = create(tmp.path(), "a.txt", "file").unwrap();
        assert_eq!(f, tmp.path().join("a.txt"));
        assert_eq!(fs::metadata(&f).unwrap().len(), 0);
        let d = create(tmp.path(), "sub", "dir").unwrap();
        assert!(d.is_dir());
        assert_eq!(create(tmp.path(), "a.txt", "file"), Err("already exists: a.txt".into()));
        assert_eq!(create(tmp.path(), "sub", "dir"), Err("already exists: sub".into()));
        assert_eq!(create(tmp.path(), "a.txt", "dir"), Err("already exists: a.txt".into()));
        assert!(create(tmp.path(), "con", "file").is_err());
        assert!(create(tmp.path(), "x", "link").is_err());
        assert_eq!(names(&list(tmp.path()).unwrap()), ["sub", "a.txt"]);
    }

    #[test]
    fn rename_moves_refuses_clobber_and_allows_case_only() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("readme.md"), "r").unwrap();
        fs::write(tmp.path().join("other.md"), "o").unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("a.rs"), "a").unwrap();

        let moved = rename(&tmp.path().join("readme.md"), "notes.md").unwrap();
        assert_eq!(moved, tmp.path().join("notes.md"));
        assert_eq!(fs::read_to_string(&moved).unwrap(), "r");
        assert!(!tmp.path().join("readme.md").exists());

        assert_eq!(rename(&tmp.path().join("notes.md"), "other.md"), Err("already exists: other.md".into()));
        assert_eq!(fs::read_to_string(tmp.path().join("other.md")).unwrap(), "o");
        assert_eq!(fs::read_to_string(tmp.path().join("notes.md")).unwrap(), "r");

        let cased = rename(&tmp.path().join("notes.md"), "NOTES.md").unwrap();
        assert!(names(&list(tmp.path()).unwrap()).contains(&"NOTES.md"));
        assert_eq!(fs::read_to_string(&cased).unwrap(), "r");

        let dir = rename(&tmp.path().join("src"), "lib").unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "a");
        assert!(rename(&tmp.path().join("missing"), "x").is_err());
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
        assert!(f.bom);
        assert_eq!(f.mtime, mtime_ms(&fs::metadata(&p).unwrap()));
        assert!(f.mtime > 1_600_000_000_000);

        fs::write(&p, b"plain").unwrap();
        assert!(!read(&p).unwrap().bom);
    }

    #[test]
    fn write_restores_crlf_and_bom_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        let w = write(&p, "one\ntwo\n", true, true, None).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"\xEF\xBB\xBFone\r\ntwo\r\n");
        let f = read(&p).unwrap();
        assert_eq!(f.text, "one\r\ntwo\r\n");
        assert!(f.bom);
        assert_eq!((f.size, f.mtime), (w.size, w.mtime));
        assert!(!tmp.path().join(format!("a.txt{TMP_SUFFIX}")).exists());

        write(&p, "x", false, false, None).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"x");
    }

    #[test]
    fn write_checks_the_expected_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a.txt");
        fs::write(&p, b"orig").unwrap();
        let now = read(&p).unwrap().mtime;

        let w = write(&p, "ok", false, false, Some(now)).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"ok");

        let err = write(&p, "stale", false, false, Some(w.mtime.wrapping_sub(1))).unwrap_err();
        assert_eq!(err, "changed on disk");
        assert_eq!(fs::read(&p).unwrap(), b"ok", "file must be untouched");

        let gone = tmp.path().join("gone.txt");
        assert_eq!(write(&gone, "x", false, false, Some(now)).unwrap_err(), "deleted on disk");
        write(&gone, "x", false, false, None).unwrap();
        assert_eq!(fs::read(&gone).unwrap(), b"x");
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
