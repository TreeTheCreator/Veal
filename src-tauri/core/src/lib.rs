//! What the desktop app and the daemon share (plan-19). Nothing here depends
//! on Tauri, so all of it can run without a window.

pub mod agents;
pub mod proxy;
pub mod pty;
pub mod session;
pub mod sysproxy;
