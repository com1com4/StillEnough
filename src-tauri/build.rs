fn main() {
    // Windows embeds the ICO in the executable and uses that resource for
    // title bars and the tray. Rebuild it when icon assets change.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
