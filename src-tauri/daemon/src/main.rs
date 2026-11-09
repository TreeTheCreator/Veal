//! `veal-daemon.exe` (plan-19). No window and no console: the app launches it,
//! and it outlives the app.

#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::time::Duration;
use veal_core::{log, pipe, vlog};
use veal_daemon::{State, IDLE};

fn main() {
    log::init("daemon");
    std::panic::set_hook(Box::new(|info| vlog!("[daemon] panic: {info}")));

    // `--build <id>`: which binary this is, for the dev build check (plan-19 D6).
    let mut args = std::env::args().skip(1);
    let mut build = format!("release-{}", env!("CARGO_PKG_VERSION"));
    while let Some(a) = args.next() {
        if a == "--build" {
            if let Some(b) = args.next() {
                build = b;
            }
        }
    }
    let idle = std::env::var("VEAL_IDLE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(IDLE);

    let name = match pipe::pipe_name() {
        Ok(n) => n,
        Err(e) => {
            vlog!("[daemon] no pipe name: {e}");
            return;
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            vlog!("[daemon] no runtime: {e}");
            return;
        }
    };
    let state = State::new(build);
    if let Err(e) = rt.block_on(veal_daemon::run(state, &name, idle)) {
        vlog!("[daemon] stopped: {e}");
    }
}
