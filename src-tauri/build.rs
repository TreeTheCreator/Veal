fn main() {
    // tauri-build embeds icons/icon.ico as the exe/window icon but never tells cargo to
    // watch it, so a regenerated icon set would otherwise not reach the binary.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
