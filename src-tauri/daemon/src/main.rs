//! `veal-daemon.exe` (plan-19). No window and no console: the app launches it,
//! and it outlives the app.

#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::time::Duration;
use veal_core::{log, pipe, vlog};
use veal_daemon::{State, IDLE};

fn main() {
    log::init("daemon");
    std::panic::set_hook(Box::new(|info| vlog!("[daemon] panic: {info}")));
    veal_core::pty::allow_ctrl_c();

    // `--build <id>`: which binary this is, for the dev build check (plan-19 D6).
    // `--pipe <name>`: the app names the pipe it will connect to.
    // `--idle-ms <n>`: tests shorten the idle exit.
    let mut args = std::env::args().skip(1);
    let mut build = format!("release-{}", env!("CARGO_PKG_VERSION"));
    let mut pipe_arg = None;
    let mut idle = IDLE;
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--build", Some(v)) => build = v,
            ("--pipe", Some(v)) => pipe_arg = Some(v),
            ("--idle-ms", Some(v)) => idle = v.parse().map(Duration::from_millis).unwrap_or(IDLE),
            _ => {}
        }
    }

    let name = match pipe_arg.map(Ok).unwrap_or_else(pipe::pipe_name) {
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
    // Only the user's own pipe gets a tray: a test daemon on a pipe of its
    // own must not put icons in the taskbar.
    let own_pipe = pipe::pipe_name().is_ok_and(|n| n == name);
    let tray_state = std::sync::Arc::clone(&state);
    let on_listening = move || {
        if own_pipe {
            veal_daemon::tray::start(tray_state);
        }
    };
    if let Err(e) = rt.block_on(veal_daemon::run(state, &name, idle, on_listening)) {
        vlog!("[daemon] stopped: {e}");
    }
}
