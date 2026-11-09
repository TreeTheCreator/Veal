// Ask for Common Controls v6 in the daemon's manifest. In a workspace build,
// Tauri switches on muda's `common-controls-v6` feature for the tray's menu,
// and without this manifest Windows refuses to load the exe
// (STATUS_ENTRYPOINT_NOT_FOUND, 0xC0000139).
fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && target == "msvc" {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
