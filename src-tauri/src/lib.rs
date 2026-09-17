mod db;
mod pty;

use db::{Db, Project};
use pty::{PtyRegistry, SpawnOptions};
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(PtyRegistry::default()))
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
        ])
        .on_window_event(|window, event| {
            // Kill every shell/agent before the window goes, so nothing is orphaned.
            if let WindowEvent::CloseRequested { .. } = event {
                window.state::<Arc<PtyRegistry>>().kill_all();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
