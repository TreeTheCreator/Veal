//! Per-project task files: `<root>/.veal/tasks.json` is the source of truth
//! (plan-05). Every mutation is a read-modify-write of the whole file, so an
//! edit an agent made in a pane is merged rather than clobbered, and the
//! write is atomic (tmp + rename). Unknown fields round-trip untouched.
//!
//! Tauri-agnostic like `fs.rs`: the free functions take a root `&Path` and
//! `cargo test` covers them with a tempdir. The Inbox (tasks with no project)
//! lives in `db.rs` and returns the same `Task` shape.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FILE_VERSION: u32 = 1;
pub const DIR_NAME: &str = ".veal";
pub const FILE_NAME: &str = "tasks.json";

/// Serialises read-modify-write across commands; managed by Tauri.
#[derive(Default)]
pub struct TasksLock(pub Mutex<()>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Todo,
    Doing,
    Done,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::Doing => "doing",
            Status::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "todo" => Ok(Status::Todo),
            "doing" => Ok(Status::Doing),
            "done" => Ok(Status::Done),
            other => Err(format!("unknown status \"{other}\"")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub created: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Task {
    pub fn new(id: String, title: String, status: Status, created: String) -> Self {
        Self {
            id,
            title,
            status,
            created,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TasksFile {
    version: u32,
    tasks: Vec<Task>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl TasksFile {
    fn empty() -> Self {
        Self {
            version: FILE_VERSION,
            tasks: Vec::new(),
            extra: Map::new(),
        }
    }
}

pub fn file_path(root: &Path) -> PathBuf {
    root.join(DIR_NAME).join(FILE_NAME)
}

// ---- reading ----

/// Parses and validates the document; a missing file is an empty one.
fn read_file(root: &Path) -> Result<TasksFile, String> {
    let path = file_path(root);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(TasksFile::empty()),
        Err(e) => return Err(format!("{FILE_NAME}: {e}")),
    };
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Result<TasksFile, String> {
    // Validate the raw shape first so a bad status names its task rather
    // than surfacing as serde's "unknown variant".
    let raw: Value = serde_json::from_slice(bytes).map_err(|e| format!("{FILE_NAME}: {e}"))?;
    let version = raw.get("version").and_then(Value::as_u64);
    if version != Some(FILE_VERSION as u64) {
        return Err(match version {
            Some(v) => format!("{FILE_NAME} is version {v}; this Veal reads version {FILE_VERSION}"),
            None => format!("{FILE_NAME}: missing \"version\""),
        });
    }
    let tasks = raw
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{FILE_NAME}: \"tasks\" must be an array"))?;
    for (i, t) in tasks.iter().enumerate() {
        let id = t.get("id").and_then(Value::as_str);
        let label = id.map(|s| format!("task \"{s}\"")).unwrap_or_else(|| format!("task #{}", i + 1));
        if id.is_none() {
            return Err(format!("{FILE_NAME}: {label}: missing \"id\""));
        }
        if t.get("title").and_then(Value::as_str).is_none() {
            return Err(format!("{FILE_NAME}: {label}: missing \"title\""));
        }
        let status = t
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{FILE_NAME}: {label}: missing \"status\""))?;
        Status::parse(status).map_err(|e| format!("{FILE_NAME}: {label}: {e}"))?;
        if t.get("created").and_then(Value::as_str).is_none() {
            return Err(format!("{FILE_NAME}: {label}: missing \"created\""));
        }
    }
    let file: TasksFile = serde_json::from_value(raw).map_err(|e| format!("{FILE_NAME}: {e}"))?;
    let mut seen = std::collections::HashSet::new();
    for t in &file.tasks {
        if !seen.insert(t.id.as_str()) {
            return Err(format!("{FILE_NAME}: duplicate id \"{}\"", t.id));
        }
    }
    Ok(file)
}

// ---- writing ----

/// One task per line so a git diff is one line per changed task.
fn render(file: &TasksFile) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"version\": {},\n", file.version));
    out.push_str("  \"tasks\": [");
    if !file.tasks.is_empty() {
        out.push('\n');
        let lines: Vec<String> = file.tasks.iter().map(|t| format!("    {}", render_task(t))).collect();
        out.push_str(&lines.join(",\n"));
        out.push('\n');
        out.push_str("  ");
    }
    out.push(']');
    for (k, v) in &file.extra {
        out.push_str(&format!(",\n  {}: {}", json_str(k), v));
    }
    out.push_str("\n}\n");
    out
}

/// `{ "id": …, "title": …, "status": …, "created": …, <extras> }` on one
/// line. Serde's compact form has no spaces, and serialising through a
/// `Value` would sort the keys, so the known ones are written by hand.
fn render_task(t: &Task) -> String {
    let mut fields = vec![
        format!("\"id\": {}", json_str(&t.id)),
        format!("\"title\": {}", json_str(&t.title)),
        format!("\"status\": {}", json_str(t.status.as_str())),
        format!("\"created\": {}", json_str(&t.created)),
    ];
    fields.extend(t.extra.iter().map(|(k, v)| format!("{}: {}", json_str(k), v)));
    format!("{{ {} }}", fields.join(", "))
}

fn json_str(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

fn write_file(root: &Path, file: &TasksFile) -> Result<(), String> {
    let path = file_path(root);
    let dir = path.parent().expect("file has a parent");
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    std::fs::write(&tmp, render(file)).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))
}

// ---- ids ----

/// Eight hex chars of the nanosecond clock; the file holds tens of tasks,
/// and `add_with_ids` retries on the vanishing chance of a collision.
pub fn fresh_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:08x}", (nanos & 0xffff_ffff) as u32)
}

pub fn unused_id(taken: &[&str], mut ids: impl Iterator<Item = String>) -> Result<String, String> {
    ids.find(|id| !taken.contains(&id.as_str()))
        .ok_or_else(|| "could not allocate a task id".to_string())
}

// ---- operations ----

pub fn list(root: &Path) -> Result<Vec<Task>, String> {
    read_file(root).map(|f| f.tasks)
}

pub fn add(root: &Path, title: &str, created: &str) -> Result<Vec<Task>, String> {
    add_with_ids(root, title, created, std::iter::repeat_with(fresh_id))
}

pub fn add_with_ids(
    root: &Path,
    title: &str,
    created: &str,
    ids: impl Iterator<Item = String>,
) -> Result<Vec<Task>, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("empty title".into());
    }
    let mut file = read_file(root)?;
    let taken: Vec<&str> = file.tasks.iter().map(|t| t.id.as_str()).collect();
    let id = unused_id(&taken, ids)?;
    file.tasks
        .push(Task::new(id, title.to_string(), Status::Todo, created.to_string()));
    write_file(root, &file)?;
    Ok(file.tasks)
}

pub fn set_status(root: &Path, id: &str, status: Status) -> Result<Vec<Task>, String> {
    let mut file = read_file(root)?;
    let task = file
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("no task \"{id}\""))?;
    task.status = status;
    write_file(root, &file)?;
    Ok(file.tasks)
}

/// Plan-10: a new title; status, `created` and extras are untouched.
pub fn set_title(root: &Path, id: &str, title: &str) -> Result<Vec<Task>, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("empty title".into());
    }
    let mut file = read_file(root)?;
    let task = file
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("no task \"{id}\""))?;
    task.title = title.to_string();
    write_file(root, &file)?;
    Ok(file.tasks)
}

pub fn remove(root: &Path, id: &str) -> Result<Vec<Task>, String> {
    let mut file = read_file(root)?;
    let before = file.tasks.len();
    file.tasks.retain(|t| t.id != id);
    if file.tasks.len() == before {
        return Err(format!("no task \"{id}\""));
    }
    write_file(root, &file)?;
    Ok(file.tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ids<'a>(n: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        n.iter().map(|s| s.to_string())
    }

    fn read(root: &Path) -> String {
        fs::read_to_string(file_path(root)).unwrap()
    }

    #[test]
    fn list_without_a_file_is_empty_and_creates_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list(tmp.path()).unwrap().is_empty());
        assert!(!tmp.path().join(DIR_NAME).exists());
    }

    #[test]
    fn add_writes_one_task_per_line() {
        let tmp = tempfile::tempdir().unwrap();
        add_with_ids(tmp.path(), "  First ", "2026-09-17", ids(&["aaaa0001"])).unwrap();
        let tasks = add_with_ids(tmp.path(), "Second", "2026-09-17", ids(&["aaaa0002"])).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            read(tmp.path()),
            "{\n  \"version\": 1,\n  \"tasks\": [\n    \
             { \"id\": \"aaaa0001\", \"title\": \"First\", \"status\": \"todo\", \"created\": \"2026-09-17\" },\n    \
             { \"id\": \"aaaa0002\", \"title\": \"Second\", \"status\": \"todo\", \"created\": \"2026-09-17\" }\n  ]\n}\n"
        );
        assert!(!tmp.path().join(DIR_NAME).join(format!("{FILE_NAME}.tmp")).exists());
    }

    #[test]
    fn fresh_ids_differ_and_are_eight_hex() {
        let tmp = tempfile::tempdir().unwrap();
        add(tmp.path(), "a", "2026-09-17").unwrap();
        let tasks = add(tmp.path(), "b", "2026-09-17").unwrap();
        assert_ne!(tasks[0].id, tasks[1].id);
        assert!(tasks.iter().all(|t| t.id.len() == 8 && t.id.chars().all(|c| c.is_ascii_hexdigit())));
    }

    #[test]
    fn set_status_and_remove_keep_order() {
        let tmp = tempfile::tempdir().unwrap();
        for (id, title) in [("a", "A"), ("b", "B"), ("c", "C")] {
            add_with_ids(tmp.path(), title, "2026-09-17", ids(&[id])).unwrap();
        }
        let tasks = set_status(tmp.path(), "a", Status::Done).unwrap();
        assert_eq!(tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(tasks[0].status, Status::Done);
        let tasks = remove(tmp.path(), "b").unwrap();
        assert_eq!(tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        let tasks = remove(tmp.path(), "a").unwrap();
        remove(tmp.path(), "c").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(read(tmp.path()), "{\n  \"version\": 1,\n  \"tasks\": []\n}\n");
    }

    #[test]
    fn unknown_fields_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(DIR_NAME)).unwrap();
        fs::write(
            file_path(tmp.path()),
            r#"{ "owner": "triyan", "version": 1, "tasks": [
                { "id": "t1", "title": "x", "status": "todo", "created": "2026-09-01", "notes": "from the shell", "a": [1, 2] }
            ] }"#,
        )
        .unwrap();
        let tasks = set_status(tmp.path(), "t1", Status::Doing).unwrap();
        assert_eq!(tasks[0].extra["notes"], "from the shell");
        assert_eq!(
            read(tmp.path()),
            "{\n  \"version\": 1,\n  \"tasks\": [\n    \
             { \"id\": \"t1\", \"title\": \"x\", \"status\": \"doing\", \"created\": \"2026-09-01\", \"a\": [1,2], \"notes\": \"from the shell\" }\n  ],\n  \
             \"owner\": \"triyan\"\n}\n"
        );
    }

    #[test]
    fn broken_files_are_refused_and_never_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(DIR_NAME)).unwrap();
        let cases: [(&str, &str); 5] = [
            ("{ \"version\": 1, \"tasks\": [ { \"id\": \"a\" \"title\": \"x\" } ] }", "line 1"),
            ("{ \"version\": 2, \"tasks\": [] }", "is version 2; this Veal reads version 1"),
            (
                "{ \"version\": 1, \"tasks\": [ { \"id\": \"t3\", \"title\": \"x\", \"status\": \"blocked\", \"created\": \"d\" } ] }",
                "task \"t3\": unknown status \"blocked\"",
            ),
            (
                "{ \"version\": 1, \"tasks\": [ { \"id\": \"t1\", \"title\": \"x\", \"status\": \"todo\", \"created\": \"d\" }, { \"id\": \"t1\", \"title\": \"y\", \"status\": \"todo\", \"created\": \"d\" } ] }",
                "duplicate id \"t1\"",
            ),
            ("{ \"version\": 1, \"tasks\": [ { \"title\": \"x\", \"status\": \"todo\", \"created\": \"d\" } ] }", "task #1: missing \"id\""),
        ];
        for (body, expect) in cases {
            fs::write(file_path(tmp.path()), body).unwrap();
            let err = list(tmp.path()).unwrap_err();
            assert!(err.contains(expect), "{err} should mention {expect}");
            let err = add(tmp.path(), "new", "2026-09-17").unwrap_err();
            assert!(err.contains(expect), "{err}");
            assert_eq!(read(tmp.path()), body, "file must be untouched");
        }
    }

    #[test]
    fn set_title_keeps_everything_else() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(DIR_NAME)).unwrap();
        fs::write(
            file_path(tmp.path()),
            "{ \"version\": 1, \"tasks\": [ { \"id\": \"t1\", \"title\": \"old\", \"status\": \"doing\", \"created\": \"2026-09-01\", \"notes\": \"n\" } ] }",
        )
        .unwrap();
        let tasks = set_title(tmp.path(), "t1", "  new title ").unwrap();
        assert_eq!(tasks[0].title, "new title");
        assert_eq!(tasks[0].status, Status::Doing);
        assert_eq!(tasks[0].created, "2026-09-01");
        assert_eq!(tasks[0].extra["notes"], "n");
        assert!(read(tmp.path()).contains("\"title\": \"new title\", \"status\": \"doing\", \"created\": \"2026-09-01\", \"notes\": \"n\""));

        let before = read(tmp.path());
        assert_eq!(set_title(tmp.path(), "t1", "  ").unwrap_err(), "empty title");
        assert_eq!(set_title(tmp.path(), "zz", "x").unwrap_err(), "no task \"zz\"");
        assert_eq!(read(tmp.path()), before, "file untouched on error");
    }

    #[test]
    fn unknown_id_and_empty_title_are_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(set_status(tmp.path(), "zz", Status::Done).unwrap_err(), "no task \"zz\"");
        assert_eq!(remove(tmp.path(), "zz").unwrap_err(), "no task \"zz\"");
        assert_eq!(add(tmp.path(), "   ", "d").unwrap_err(), "empty title");
        assert!(!tmp.path().join(DIR_NAME).exists());
    }

    #[test]
    fn colliding_ids_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        add_with_ids(tmp.path(), "a", "d", ids(&["aaaa"])).unwrap();
        let tasks = add_with_ids(tmp.path(), "b", "d", ids(&["aaaa", "aaaa", "bbbb"])).unwrap();
        assert_eq!(tasks[1].id, "bbbb");
    }
}
