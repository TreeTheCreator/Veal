//! The tray icon (plan-19 section 8). It shows while shells are running and no
//! window is attached, so a user can tell that something is still running. It
//! runs on its own thread with a Win32 message loop and polls `tray_view` once
//! a second (E2). Nothing here may take down the sessions: every failure is
//! logged and the next tick tries again.

use crate::State;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use veal_core::vlog;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MessageBoxW, SetTimer, TranslateMessage, IDYES, MB_DEFBUTTON2,
    MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MSG, WM_TIMER,
};

/// `icons/32x32.png` as raw RGBA, so the daemon needs no PNG decoder.
/// Made with PIL: `Image.open(p).convert("RGBA").tobytes()`.
const ICON_RGBA: &[u8] = include_bytes!("tray-32.rgba");
const ICON_SIZE: u32 = 32;

const OPEN: &str = "open";
const STOP: &str = "stop";

pub fn start(state: Arc<State>) {
    let spawned = std::thread::Builder::new().name("tray".into()).spawn(move || run(state));
    if let Err(e) = spawned {
        vlog!("[tray] no thread: {e}");
    }
}

fn run(state: Arc<State>) {
    {
        let state = Arc::clone(&state);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| match e.id.as_ref() {
            OPEN => open_app(),
            STOP => stop(&state),
            _ => {}
        }));
    }
    TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            open_app();
        }
    }));

    // A thread timer: no window, so WM_TIMER arrives with a null hwnd.
    unsafe { SetTimer(std::ptr::null_mut(), 0, 1000, None) };
    let mut icon: Option<TrayIcon> = None;
    let mut shown = String::new();
    let mut failed = false;
    tick(&state, &mut icon, &mut shown, &mut failed);
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            break;
        }
        if msg.message == WM_TIMER && msg.hwnd.is_null() {
            tick(&state, &mut icon, &mut shown, &mut failed);
            continue;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    vlog!("[tray] message loop ended");
}

fn tick(state: &State, icon: &mut Option<TrayIcon>, shown: &mut String, failed: &mut bool) {
    let Some(view) = state.tray_view() else {
        if icon.take().is_some() {
            vlog!("[tray] hidden");
        }
        return;
    };
    match icon {
        Some(i) => {
            if *shown != view.tooltip {
                if let Err(e) = i.set_tooltip(Some(&view.tooltip)) {
                    vlog!("[tray] tooltip: {e}");
                }
                *shown = view.tooltip;
            }
        }
        None => match build(&view.tooltip) {
            Ok(i) => {
                vlog!("[tray] shown: {}", view.tooltip);
                *icon = Some(i);
                *shown = view.tooltip;
                *failed = false;
            }
            Err(e) => {
                // Once, not every second.
                if !*failed {
                    vlog!("[tray] could not show: {e}");
                }
                *failed = true;
            }
        },
    }
}

fn build(tooltip: &str) -> Result<TrayIcon, String> {
    let menu = Menu::new();
    let open = MenuItem::with_id(MenuId::new(OPEN), "Open Veal", true, None);
    let stop = MenuItem::with_id(MenuId::new(STOP), "Stop everything and quit", true, None);
    menu.append(&open).map_err(|e| e.to_string())?;
    menu.append(&stop).map_err(|e| e.to_string())?;
    let image = Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE).map_err(|e| e.to_string())?;
    TrayIconBuilder::new()
        .with_tooltip(tooltip)
        .with_icon(image)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|e| e.to_string())
}

/// `veal.exe` from next to this exe (E4). In dev the daemon is a copy in
/// `target\debug`, beside the app. A second launch focuses the running window.
fn open_app() {
    let exe = match std::env::current_exe() {
        Ok(p) => p.with_file_name("veal.exe"),
        Err(e) => {
            vlog!("[tray] no exe path: {e}");
            return;
        }
    };
    let mut cmd = std::process::Command::new(&exe);
    if let Some(home) = std::env::var_os("USERPROFILE") {
        cmd.current_dir(home);
    }
    match cmd.spawn() {
        Ok(child) => vlog!("[tray] opened {} (pid {})", exe.display(), child.id()),
        Err(e) => vlog!("[tray] could not open {}: {e}", exe.display()),
    }
}

/// Asks first when agents are running (E6), then ends everything.
fn stop(state: &Arc<State>) {
    let agents = state.registry.agents().len();
    if agents > 0 {
        let s = if agents == 1 { "agent is" } else { "agents are" };
        let text = wide(&format!("{agents} {s} running. Stop everything and quit?"));
        let title = wide("Veal");
        let answer = unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND | MB_TOPMOST,
            )
        };
        if answer != IDYES {
            return;
        }
    }
    vlog!("[tray] stop everything");
    let state = Arc::clone(state);
    let _ = std::thread::Builder::new().name("tray-stop".into()).spawn(move || state.shutdown());
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
