//! SQLite persistence: pinned projects, a key/value settings table (which
//! also holds the serialised pane layout) and the task Inbox — tasks with no
//! project, the only tasks not kept in a `.veal/tasks.json` (plan-05). One
//! connection behind a mutex; the database is a few dozen rows, so nothing
//! here is async.
//!
//! Tauri-agnostic like `pty.rs`: `Db::open_in_memory` lets `cargo test` cover
//! the migration and every query without a webview.

use crate::tasks::{self, Status, Task};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// Bumped whenever `migrate` gains a step.
const SCHEMA_VERSION: u32 = 2;

pub const KEY_ACTIVE_PROJECT: &str = "active_project";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Project {
    pub id: u32,
    pub name: String,
    pub path: String,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn sql_err(e: rusqlite::Error) -> String {
    e.to_string()
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        }
        let conn = Connection::open(path).map_err(sql_err)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(sql_err)?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(sql_err)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[cfg(test)]
    pub fn schema_version(&self) -> Result<u32, String> {
        self.lock()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql_err)
    }

    // ---- projects ----

    pub fn projects_list(&self) -> Result<Vec<Project>, String> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT id, name, path FROM projects ORDER BY position")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Project {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: r.get(2)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    /// Pins a project, creating its directory if it does not exist. Appends
    /// to the end of the sidebar.
    pub fn projects_add(&self, name: &str, path: &str) -> Result<Project, String> {
        let path = normalize_path(path);
        if path.is_empty() || name.trim().is_empty() {
            return Err("name and path are required".into());
        }
        let p = Path::new(&path);
        if p.is_file() {
            return Err(format!("not a directory: {path}"));
        }
        std::fs::create_dir_all(p).map_err(|e| format!("create {path}: {e}"))?;

        let conn = self.lock();
        let position: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM projects", [], |r| r.get(0))
            .map_err(sql_err)?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO projects (name, path, position) VALUES (?1, ?2, ?3)",
            params![name.trim(), path, position],
        );
        match inserted {
            Ok(1) => Ok(Project {
                id: conn.last_insert_rowid() as u32,
                name: name.trim().to_string(),
                path,
            }),
            Ok(_) => Err(format!("already pinned: {path}")),
            Err(e) => Err(sql_err(e)),
        }
    }

    /// Unpins a project. Clears `active_project` if it pointed here.
    pub fn projects_remove(&self, id: u32) -> Result<(), String> {
        let conn = self.lock();
        conn.execute("DELETE FROM projects WHERE id = ?1", [id])
            .map_err(sql_err)?;
        conn.execute(
            "DELETE FROM settings WHERE key = ?1 AND value = ?2",
            params![KEY_ACTIVE_PROJECT, id.to_string()],
        )
        .map_err(sql_err)?;
        Ok(())
    }

    // ---- settings ----

    pub fn settings_get_all(&self) -> Result<HashMap<String, String>, String> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    #[cfg(test)]
    pub fn settings_get(&self, key: &str) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.lock()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(sql_err)
    }

    pub fn settings_set(&self, key: &str, value: &str) -> Result<(), String> {
        self.lock()
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(sql_err)
    }

    // ---- inbox ----
    // Mirrors tasks.rs: every mutation returns the whole list, ids come from
    // the same generator, so the frontend has one Task type for both.

    fn inbox_list_locked(conn: &Connection) -> Result<Vec<Task>, String> {
        let mut stmt = conn
            .prepare("SELECT id, title, status, created FROM tasks ORDER BY position")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_err)?;
        rows.map(|row| {
            let (id, title, status, created) = row.map_err(sql_err)?;
            let status = Status::parse(&status).map_err(|e| format!("inbox task \"{id}\": {e}"))?;
            Ok(Task::new(id, title, status, created))
        })
        .collect()
    }

    pub fn inbox_list(&self) -> Result<Vec<Task>, String> {
        Self::inbox_list_locked(&self.lock())
    }

    pub fn inbox_add(&self, title: &str, created: &str) -> Result<Vec<Task>, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("empty title".into());
        }
        let conn = self.lock();
        let taken: Vec<String> = Self::inbox_list_locked(&conn)?.into_iter().map(|t| t.id).collect();
        let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
        let id = tasks::unused_id(&taken, std::iter::repeat_with(tasks::fresh_id))?;
        let position: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM tasks", [], |r| r.get(0))
            .map_err(sql_err)?;
        conn.execute(
            "INSERT INTO tasks (id, title, status, created, position) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, Status::Todo.as_str(), created, position],
        )
        .map_err(sql_err)?;
        Self::inbox_list_locked(&conn)
    }

    pub fn inbox_set_status(&self, id: &str, status: Status) -> Result<Vec<Task>, String> {
        let conn = self.lock();
        let changed = conn
            .execute("UPDATE tasks SET status = ?1 WHERE id = ?2", params![status.as_str(), id])
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(format!("no task \"{id}\""));
        }
        Self::inbox_list_locked(&conn)
    }

    pub fn inbox_set_title(&self, id: &str, title: &str) -> Result<Vec<Task>, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("empty title".into());
        }
        let conn = self.lock();
        let changed = conn
            .execute("UPDATE tasks SET title = ?1 WHERE id = ?2", params![title, id])
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(format!("no task \"{id}\""));
        }
        Self::inbox_list_locked(&conn)
    }

    pub fn inbox_remove(&self, id: &str) -> Result<Vec<Task>, String> {
        let conn = self.lock();
        let changed = conn
            .execute("DELETE FROM tasks WHERE id = ?1", [id])
            .map_err(sql_err)?;
        if changed == 0 {
            return Err(format!("no task \"{id}\""));
        }
        Self::inbox_list_locked(&conn)
    }
}

/// Strips trailing separators so `C:\x\` and `C:\x` are the same pin, and on
/// Windows turns `/` into `\` so a typed `C:/x` matches a picked `C:\x`.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['\\', '/']);
    let owned;
    let trimmed = if cfg!(windows) {
        owned = trimmed.replace('/', "\\");
        owned.as_str()
    } else {
        trimmed
    };
    // Keep a bare drive root (`C:\`) intact.
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}

fn migrate(conn: &Connection) -> Result<(), String> {
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(sql_err)?;
    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE projects (
                id       INTEGER PRIMARY KEY,
                name     TEXT NOT NULL,
                path     TEXT NOT NULL UNIQUE,
                position INTEGER NOT NULL
             );
             CREATE TABLE settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
             );",
        )
        .map_err(sql_err)?;
    }
    if version < 2 {
        conn.execute_batch(
            "CREATE TABLE tasks (
                id       TEXT PRIMARY KEY,
                title    TEXT NOT NULL,
                status   TEXT NOT NULL,
                created  TEXT NOT NULL,
                position INTEGER NOT NULL
             );",
        )
        .map_err(sql_err)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(sql_err)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("veal-db-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn migration_lands_at_current_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(db.projects_list().unwrap().is_empty());
    }

    #[test]
    fn add_list_roundtrip_creates_dir_and_rejects_duplicates() {
        let db = Db::open_in_memory().unwrap();
        let dir = tmp_dir("add");
        let with_slash = format!("{dir}\\");
        let p = db.projects_add("scratch", &with_slash).unwrap();
        assert!(Path::new(&dir).is_dir(), "directory should be created");
        assert_eq!(p.path, dir, "trailing separator is stripped");
        assert_eq!(db.projects_list().unwrap(), vec![p.clone()]);

        let err = db.projects_add("again", &dir).unwrap_err();
        assert_eq!(err, format!("already pinned: {dir}"));
        assert_eq!(db.projects_list().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_is_in_pin_order() {
        let db = Db::open_in_memory().unwrap();
        let a = tmp_dir("order-a");
        let b = tmp_dir("order-b");
        db.projects_add("b", &b).unwrap();
        db.projects_add("a", &a).unwrap();
        let names: Vec<_> = db.projects_list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["b", "a"]);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn remove_clears_active_project() {
        let db = Db::open_in_memory().unwrap();
        let dir = tmp_dir("remove");
        let p = db.projects_add("x", &dir).unwrap();
        db.settings_set(KEY_ACTIVE_PROJECT, &p.id.to_string()).unwrap();
        db.projects_remove(p.id).unwrap();
        assert!(db.projects_list().unwrap().is_empty());
        assert_eq!(db.settings_get(KEY_ACTIVE_PROJECT).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_upsert_overwrites() {
        let db = Db::open_in_memory().unwrap();
        db.settings_set("shell", "pwsh").unwrap();
        db.settings_set("shell", "cmd").unwrap();
        let all = db.settings_get_all().unwrap();
        assert_eq!(all.get("shell").map(String::as_str), Some("cmd"));
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn v1_database_migrates_to_v2_keeping_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE, position INTEGER NOT NULL);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO projects (name, path, position) VALUES ('veal', 'C:\\veal', 1);
             INSERT INTO settings (key, value) VALUES ('shell', 'pwsh');
             PRAGMA user_version = 1;",
        )
        .unwrap();
        let db = Db::init(conn).unwrap();
        assert_eq!(db.schema_version().unwrap(), 2);
        assert_eq!(db.projects_list().unwrap()[0].name, "veal");
        assert_eq!(db.settings_get("shell").unwrap().as_deref(), Some("pwsh"));
        assert!(db.inbox_list().unwrap().is_empty());
    }

    #[test]
    fn inbox_roundtrip_keeps_position_order() {
        let db = Db::open_in_memory().unwrap();
        db.inbox_add("first", "2026-09-17").unwrap();
        db.inbox_add("second", "2026-09-17").unwrap();
        let tasks = db.inbox_add("  third ", "2026-09-17").unwrap();
        let titles: Vec<_> = tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["first", "second", "third"]);
        assert!(tasks.iter().all(|t| t.status == Status::Todo && t.id.len() == 8));

        let tasks = db.inbox_set_status(&tasks[0].id, Status::Done).unwrap();
        assert_eq!(tasks[0].status, Status::Done);
        assert_eq!(tasks[0].title, "first");
        let tasks = db.inbox_set_title(&tasks[0].id, " renamed ").unwrap();
        assert_eq!(tasks[0].title, "renamed");
        assert_eq!(tasks[0].status, Status::Done);
        assert_eq!(db.inbox_set_title(&tasks[0].id, " ").unwrap_err(), "empty title");
        assert_eq!(db.inbox_set_title("zz", "x").unwrap_err(), "no task \"zz\"");
        let tasks = db.inbox_remove(&tasks[1].id).unwrap();
        let titles: Vec<_> = tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["renamed", "third"]);

        assert_eq!(db.inbox_set_status("zz", Status::Done).unwrap_err(), "no task \"zz\"");
        assert_eq!(db.inbox_remove("zz").unwrap_err(), "no task \"zz\"");
        assert_eq!(db.inbox_add(" ", "d").unwrap_err(), "empty title");
    }

    #[test]
    fn add_rejects_a_file_path() {
        let db = Db::open_in_memory().unwrap();
        let f = std::env::temp_dir().join(format!("veal-db-test-file-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        let err = db.projects_add("f", &f.to_string_lossy()).unwrap_err();
        assert!(err.starts_with("not a directory"), "{err}");
        let _ = std::fs::remove_file(&f);
    }
}
