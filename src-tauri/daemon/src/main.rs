//! `veal-daemon.exe` (plan-19). No window and no console: the app launches it,
//! and it outlives the app.

#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::time::Duration;
use veal_core::client::Stopped;
use veal_core::{log, pipe, vlog};
use veal_daemon::{State, IDLE};

fn main() {
    log::init("daemon");
    std::panic::set_hook(Box::new(|info| vlog!("[daemon] panic: {info}")));
    veal_core::pty::allow_ctrl_c();

    // `--build <id>`: which binary this is, for the dev build check (plan-19 D6).
    // `--pipe <name>`: the app names the pipe it will connect to.
    // `--idle-ms <n>`: tests shorten the idle exit.
    // `--stop`: end the running daemon and exit, for the installer (plan-19 7.3).
    let mut build = format!("release-{}", env!("CARGO_PKG_VERSION"));
    let mut pipe_arg = None;
    let mut idle = IDLE;
    let mut stop = false;
    let mut protocol = veal_core::protocol::PROTOCOL;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--stop" => stop = true,
            // Tests only: answer `hello` as another version would (plan-19 7.5).
            "--protocol" if cfg!(debug_assertions) => {
                protocol = args.next().and_then(|v| v.parse().ok()).unwrap_or(protocol)
            }
            "--build" => build = args.next().unwrap_or(build),
            "--pipe" => pipe_arg = args.next(),
            "--idle-ms" => idle = args.next().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(IDLE),
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
    if stop {
        std::process::exit(stop_daemon(&name));
    }
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            vlog!("[daemon] no runtime: {e}");
            return;
        }
    };
    let state = State::with_protocol(build, protocol);
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

/// Exit codes for `--stop`: 0 stopped, 1 nothing was running, 2 still running.
fn stop_daemon(name: &str) -> i32 {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            vlog!("[stop] no runtime: {e}");
            return 2;
        }
    };
    let result = rt.block_on(veal_core::client::stop(name));
    vlog!("[stop] {name}: {result:?}");
    match result {
        Stopped::Stopped => 0,
        Stopped::NotRunning => 1,
        Stopped::StillRunning => 2,
    }
}
