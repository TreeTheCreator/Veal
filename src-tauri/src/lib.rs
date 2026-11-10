mod bridge;
mod db;
mod fs;
mod tasks;
mod watch;

use db::{Db, Project};
use tasks::{Status, Task, TasksLock};
use bridge::{Bridge, OpenedPane};
use veal_core::protocol::SessionInfo;
use veal_core::proxy;
use watch::Watchers;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Response;
use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, State, WindowEvent};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchError {
    project_id: u32,
    error: String,
}

/// Watches a pinned root; a failure is an event, not a command error — the
/// pin itself stands, the project is just not live (plan-07, decision 7).
fn watch_root(app: &tauri::AppHandle, watchers: &Watchers, project: &Project) {
    if let Err(error) = watchers.watch(project.id, std::path::Path::new(&project.path)) {
        let _ = app.emit("fs-watch-error", WatchError { project_id: project.id, error });
    }
}

/// One path for a pane's spawn and the Settings row, so Settings shows
/// what a new pane gets (plan-18). A missing or unknown mode is the
/// default, Follow system. Blocks for the liveness check: at most two
/// 200 ms connects, only when Windows names a loopback proxy.
fn resolve_proxy(mode: Option<&str>, port: Option<&str>) -> proxy::Resolved {
    proxy::resolve_setting(mode, port)
}

/// The Settings row: the decision and the exact variables a new pane gets.
#[derive(Serialize)]
struct ProxyPreview {
    #[serde(flatten)]
    resolved: proxy::Resolved,
    env: Vec<(String, String)>,
}

#[tauri::command(async)]
fn proxy_resolve(proxy_mode: Option<String>, proxy_port: Option<String>, proxy_bypass: Option<String>) -> ProxyPreview {
    let resolved = resolve_proxy(proxy_mode.as_deref(), proxy_port.as_deref());
    let env = proxy::proxy_env(&resolved, proxy_bypass.as_deref());
    ProxyPreview { resolved, env }
}

/// A dev build passes its own environment on to panes, which otherwise get a
/// fresh user environment and would lose the dev shell's variables (plan-19 5.3).
fn dev_overlay() -> Option<Vec<(String, String)>> {
    cfg!(debug_assertions).then(|| std::env::vars().collect())
}

// The shells live in `veal-daemon.exe` (plan-19 step 5). These commands keep
// the shapes they had when the shells were in-process, and forward to it
// through `bridge.rs`.

/// Attach to the pane's session, or spawn it if there is none (plan-19).
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
async fn pty_open(
    bridge: State<'_, Arc<Bridge>>,
    key: String,
    cwd: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    proxy_mode: Option<String>,
    proxy_port: Option<String>,
    proxy_bypass: Option<String>,
) -> Result<OpenedPane, String> {
    bridge.open(key, cwd, cols, rows, shell, proxy_mode, proxy_port, proxy_bypass, dev_overlay()).await
}

/// What is running, for boot to reconcile against the saved layout (plan-19).
#[tauri::command(async)]
async fn pty_list(bridge: State<'_, Arc<Bridge>>) -> Result<Vec<SessionInfo>, String> {
    bridge.list().await
}

/// The pane's terminal went away; its shell keeps running. `client` is the
/// one `pty_open` returned, which the bridge already holds.
#[tauri::command(async)]
fn pty_detach(bridge: State<'_, Arc<Bridge>>, id: u32, client: u32) {
    let _ = client;
    bridge.detach(id);
}

/// Boot found an exited session it has nothing to show for.
#[tauri::command(async)]
async fn pty_forget(bridge: State<'_, Arc<Bridge>>, key: String) -> Result<(), String> {
    bridge.forget(key).await
}

/// Raw bytes reach JS as an ArrayBuffer over the ipc protocol; xterm decodes
/// UTF-8 itself, so a take boundary inside a multibyte sequence is harmless.
#[tauri::command(async)]
fn pty_read(bridge: State<'_, Arc<Bridge>>, id: u32, client: u32) -> Response {
    let _ = client;
    Response::new(bridge.read(id))
}

// Every command is `async` so it runs on the async runtime, not the main
// thread (plan-16). A write never blocks now: it is queued on the pipe, and
// the daemon's per-session writer thread does the I/O (plan-19 D9).
#[tauri::command(async)]
fn pty_write(bridge: State<'_, Arc<Bridge>>, id: u32, data: String) -> Result<(), String> {
    bridge.write(id, data.as_bytes())
}

#[tauri::command(async)]
fn pty_resize(bridge: State<'_, Arc<Bridge>>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    bridge.resize(id, cols, rows)
}

#[tauri::command(async)]
async fn pty_kill(bridge: State<'_, Arc<Bridge>>, key: String) -> Result<(), String> {
    bridge.kill(key).await
}

/// The close guard's choice, just before the window goes: keep the shells
/// running in the background, or end them (plan-19 E3).
#[tauri::command(async)]
fn pty_close_mode(bridge: State<'_, Arc<Bridge>>, keep: bool) {
    bridge.set_keep_on_close(keep);
}

/// An update is about to install: end every shell and the daemon, and wait
/// for it to go. Whether it went (plan-19 7.4).
#[tauri::command(async)]
async fn pty_shutdown(bridge: State<'_, Arc<Bridge>>) -> Result<bool, String> {
    Ok(bridge.stop_for_update().await)
}

/// Sessions on a background service of another version, if one was found
/// (plan-19 7.5). The prompt asks on mount, since the first connect is made
/// before the page listens for `pty-daemon-mismatch`.
#[tauri::command(async)]
fn pty_mismatch(bridge: State<'_, Arc<Bridge>>) -> Option<usize> {
    bridge.mismatch()
}

/// The mismatch prompt's Restart: end the old service and its sessions.
#[tauri::command(async)]
async fn pty_replace_daemon(bridge: State<'_, Arc<Bridge>>) -> Result<(), String> {
    bridge.replace_daemon().await
}

/// The terminal colours, for OSC 10/11 answers while no window is attached.
#[tauri::command(async)]
fn pty_theme(bridge: State<'_, Arc<Bridge>>, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
    bridge.theme(fg, bg);
}

// ---- persistence (src/db.rs) ----

#[tauri::command(async)]
fn projects_list(db: State<'_, Db>) -> Result<Vec<Project>, String> {
    db.projects_list()
}

#[tauri::command(async)]
fn projects_add(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    watchers: State<'_, Watchers>,
    name: String,
    path: String,
) -> Result<Project, String> {
    let project = db.projects_add(&name, &path)?;
    watch_root(&app, &watchers, &project);
    Ok(project)
}

#[tauri::command(async)]
fn projects_remove(db: State<'_, Db>, watchers: State<'_, Watchers>, id: u32) -> Result<(), String> {
    watchers.unwatch(id);
    db.projects_remove(id)
}

#[tauri::command(async)]
fn settings_get_all(db: State<'_, Db>) -> Result<HashMap<String, String>, String> {
    db.settings_get_all()
}

#[tauri::command(async)]
fn settings_set(db: State<'_, Db>, key: String, value: String) -> Result<(), String> {
    db.settings_set(&key, &value)
}

/// `"dir"`, `"file"` or `"missing"`; feeds the add-project hint.
#[tauri::command(async)]
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

#[tauri::command(async)]
fn fs_list(db: State<'_, Db>, dir: String) -> Result<Vec<fs::Entry>, String> {
    fs::list(&fs::confine(&db, &dir)?)
}

#[tauri::command(async)]
fn fs_read(db: State<'_, Db>, path: String) -> Result<fs::FileText, String> {
    fs::read(&fs::confine(&db, &path)?)
}

#[tauri::command(async)]
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

// The confined path is canonical (`\\?\C:\…` on Windows); JS keeps the
// path it sent, so the new path is built from that, not from the result.
#[tauri::command(async)]
fn fs_create(db: State<'_, Db>, dir: String, name: String, kind: String) -> Result<String, String> {
    fs::create(&fs::confine(&db, &dir)?, &name, &kind)?;
    Ok(std::path::Path::new(dir.trim()).join(&name).to_string_lossy().into_owned())
}

// Rename and delete act on the entry itself (plan-16): a junction or
// symlink is renamed or trashed, never what it points to.
#[tauri::command(async)]
fn fs_rename(db: State<'_, Db>, path: String, name: String) -> Result<String, String> {
    fs::rename(&fs::confine_entry(&db, &path)?, &name)?;
    let src = std::path::Path::new(path.trim());
    let parent = src.parent().ok_or_else(|| format!("no parent: {path}"))?;
    Ok(parent.join(&name).to_string_lossy().into_owned())
}

#[tauri::command(async)]
async fn fs_delete(db: State<'_, Db>, path: String) -> Result<(), String> {
    let entry = fs::confine_entry(&db, &path)?;
    tauri::async_runtime::spawn_blocking(move || fs::delete(&entry))
        .await
        .map_err(|e| e.to_string())?
}

// ---- tasks view (src/tasks.rs + the Inbox in src/db.rs) ----
// `root: None` is the Inbox; `Some` is a pinned project root. The lock
// serialises the file read-modify-write across commands.

fn task_root(db: &Db, root: Option<String>) -> Result<Option<std::path::PathBuf>, String> {
    root.map(|r| fs::confine(db, &r)).transpose()
}

#[tauri::command(async)]
fn tasks_list(db: State<'_, Db>, lock: State<'_, TasksLock>, root: Option<String>) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::list(&root),
        None => db.inbox_list(),
    }
}

#[tauri::command(async)]
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

#[tauri::command(async)]
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

#[tauri::command(async)]
fn tasks_set_title(
    db: State<'_, Db>,
    lock: State<'_, TasksLock>,
    root: Option<String>,
    id: String,
    title: String,
) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::set_title(&root, &id, &title),
        None => db.inbox_set_title(&id, &title),
    }
}

#[tauri::command(async)]
fn tasks_shift(
    db: State<'_, Db>,
    lock: State<'_, TasksLock>,
    root: Option<String>,
    id: String,
    delta: i32,
) -> Result<Vec<Task>, String> {
    let _g = lock.0.lock().unwrap_or_else(|p| p.into_inner());
    match task_root(&db, root)? {
        Some(root) => tasks::shift(&root, &id, delta),
        None => db.inbox_shift(&id, delta),
    }
}

#[tauri::command(async)]
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
        // First, as its docs require (plan-16): a second launch focuses the
        // running window and exits, instead of a second window on the same
        // database and a second set of shells. Veal takes no argv.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(TasksLock::default())
        .setup(|app| {
            veal_core::log::init("app");
            // %APPDATA%\com.veal.app\veal.db on Windows.
            let path = app.path().app_data_dir()?.join("veal.db");
            app.manage(Db::open(&path)?);
            // One coalescing thread for every pinned root; batches reach JS
            // as `fs-changed` (plan-07).
            let handle = app.handle().clone();
            let watchers = Watchers::new(move |batch| {
                let _ = handle.emit("fs-changed", batch);
            });
            for project in app.state::<Db>().projects_list()? {
                watch_root(app.handle(), &watchers, &project);
            }
            app.manage(watchers);
            // The shells live in the daemon. Connect now, so the first pane
            // does not wait for a launch; commands share this connection.
            let bridge = Arc::new(Bridge::new(app.handle().clone()));
            app.manage(Arc::clone(&bridge));
            tauri::async_runtime::spawn(async move {
                if let Err(e) = bridge.client().await {
                    veal_core::vlog!("[bridge] first connect: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pty_open,
            pty_list,
            pty_detach,
            pty_forget,
            pty_read,
            pty_write,
            pty_resize,
            pty_kill,
            pty_theme,
            pty_close_mode,
            pty_shutdown,
            pty_mismatch,
            pty_replace_daemon,
            proxy_resolve,
            projects_list,
            projects_add,
            projects_remove,
            settings_get_all,
            settings_set,
            path_kind,
            fs_list,
            fs_read,
            fs_write,
            fs_create,
            fs_rename,
            fs_delete,
            tasks_list,
            tasks_add,
            tasks_set_status,
            tasks_set_title,
            tasks_shift,
            tasks_remove,
        ])
        // A fresh page (F5, "Refresh", the dev loop's full reload) never ran the
        // old page's React cleanup, so its panes are still attached. The shells
        // stay: boot reattaches to them (plan-19).
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                webview.state::<Arc<Bridge>>().detach_all();
            }
        })
        .on_window_event(|window, event| {
            // The window is really going. Not on CloseRequested: the close guard
            // in JS (plan-06) can cancel that, and the panes must survive it.
            // The shells keep running only if the guard chose Keep running.
            if let WindowEvent::Destroyed = event {
                let bridge = Arc::clone(&*window.state::<Arc<Bridge>>());
                tauri::async_runtime::block_on(bridge.close());
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
