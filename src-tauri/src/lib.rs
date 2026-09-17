mod db;
mod fs;
mod pty;
mod tasks;

use db::{Db, Project};
use pty::{PtyRegistry, SpawnOptions};
use tasks::{Status, Task, TasksLock};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Emitter, Manager, State, WindowEvent};

#[derive(Clone, Serialize)]
struct PtyExit {
    id: u32,
    code: Option<i32>,
}

#[tauri::command]
fn pty_spawn(
    app: tauri::AppHandle,
    registry: State<'_, Arc<PtyRegistry>>,
    cwd: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    out: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    // Raw bytes reach JS as an ArrayBuffer; xterm decodes UTF-8 itself, so a
    // chunk boundary inside a multibyte sequence is harmless.
    let on_data = move |chunk: Vec<u8>| {
        let _ = out.send(InvokeResponseBody::Raw(chunk));
    };
    let on_exit = move |id: u32, code: Option<i32>| {
        let _ = app.emit("pty-exit", PtyExit { id, code });
    };
    registry.spawn(
        SpawnOptions {
            cwd,
            cols,
            rows,
            shell,
        },
        on_data,
        on_exit,
    )
}

#[tauri::command]
fn pty_write(registry: State<'_, Arc<PtyRegistry>>, id: u32, data: String) -> Result<(), String> {
    registry.write(id, data.as_bytes())
}

#[tauri::command]
fn pty_resize(
    registry: State<'_, Arc<PtyRegistry>>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    registry.resize(id, cols, rows)
}

#[tauri::command]
fn pty_kill(registry: State<'_, Arc<PtyRegistry>>, id: u32) -> Result<(), String> {
    registry.kill(id)
}

// ---- persistence (src/db.rs) ----

#[tauri::command]
fn projects_list(db: State<'_, Db>) -> Result<Vec<Project>, String> {
    db.projects_list()
}

#[tauri::command]
fn projects_add(db: State<'_, Db>, name: String, path: String) -> Result<Project, String> {
    db.projects_add(&name, &path)
}

#[tauri::command]
fn projects_remove(db: State<'_, Db>, id: u32) -> Result<(), String> {
    db.projects_remove(id)
}

#[tauri::command]
fn settings_get_all(db: State<'_, Db>) -> Result<HashMap<String, String>, String> {
    db.settings_get_all()
}

#[tauri::command]
fn settings_set(db: State<'_, Db>, key: String, value: String) -> Result<(), String> {
    db.settings_set(&key, &value)
}

/// `"dir"`, `"file"` or `"missing"`; feeds the add-project hint.
#[tauri::command]
fn path_kind(path: String) -> &'static str {
    let p = std::path::Path::new(path.trim());
    if p.is_dir() {
        "dir"
    } else if p.is_file() {
        "file"
    } else {
        "missing"
    }
}

// ---- files view (src/fs.rs) ----

#[tauri::command]
fn fs_list(db: State<'_, Db>, dir: String) -> Result<Vec<fs::Entry>, String> {
    fs::list(&fs::confine(&db, &dir)?)
}

#[tauri::command]
fn fs_read(db: State<'_, Db>, path: String) -> Result<fs::FileText, String> {
    fs::read(&fs::confine(&db, &path)?)
}

#[tauri::command]
fn fs_write(
    db: State<'_, Db>,
    path: String,
    text: String,
    crlf: bool,
    bom: bool,
    expected_mtime: Option<u64>,
) -> Result<fs::Written, String> {
    // A deleted file cannot be canonicalised, so confine its parent instead.
    let target = std::path::Path::new(path.trim());
    let confined = match fs::confine(&db, &path) {
        Ok(p) => p,
        Err(_) => {
            let parent = target.parent().ok_or_else(|| format!("no parent: {path}"))?;
            let name = target.file_name().ok_or_else(|| format!("no file name: {path}"))?;
            fs::confine(&db, &parent.to_string_lossy())?.join(name)
        }
    };
    fs::write(&confined, &text, crlf, bom, expected_mtime)
}

// ---- tasks view (src/tasks.rs + the Inbox in src/db.rs) ----
// `root: None` is the Inbox; `Some` is a pinned project root. The lock
// serialises the file read-modify-write across commands.

fn task_root(db: &Db, root: Option<String>) -> Result<Option<std::path::PathBuf>, String> {
    root.map(|r| fs::confine(db, &r)).transpose()
}

#[tauri::command]
fn tasks_list(db: State<'_, Db>, lock: State<'_, TasksLock>, root: Option<String>) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::list(&root),
        None => db.inbox_list(),
    }
}

#[tauri::command]
fn tasks_add(
    db: State<'_, Db>,
    lock: State<'_, TasksLock>,
    root: Option<String>,
    title: String,
    created: String,
) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::add(&root, &title, &created),
        None => db.inbox_add(&title, &created),
    }
}

#[tauri::command]
fn tasks_set_status(
    db: State<'_, Db>,
    lock: State<'_, TasksLock>,
    root: Option<String>,
    id: String,
    status: Status,
) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::set_status(&root, &id, status),
        None => db.inbox_set_status(&id, status),
    }
}

#[tauri::command]
fn tasks_remove(
    db: State<'_, Db>,
    lock: State<'_, TasksLock>,
    root: Option<String>,
    id: String,
) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::remove(&root, &id),
        None => db.inbox_remove(&id),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(PtyRegistry::default()))
        .manage(TasksLock::default())
        .setup(|app| {
            // %APPDATA%\com.veal.app\veal.db on Windows.
            let path = app.path().app_data_dir()?.join("veal.db");
            app.manage(Db::open(&path)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill,
            projects_list,
            projects_add,
            projects_remove,
            settings_get_all,
            settings_set,
            path_kind,
            fs_list,
            fs_read,
            fs_write,
            tasks_list,
            tasks_add,
            tasks_set_status,
            tasks_remove,
        ])
        .on_window_event(|window, event| {
            // Kill every shell/agent once the window is really going, so
            // nothing is orphaned. Not on CloseRequested: the close guard in
            // JS (plan-06) can cancel that, and the panes must survive it.
            if let WindowEvent::Destroyed = event {
                window.state::<Arc<PtyRegistry>>().kill_all();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
