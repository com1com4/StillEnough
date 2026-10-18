fn main() {
    println!("cargo:rerun-if-changed=icons");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=tests/test-manifest.rc");
        println!("cargo:rerun-if-changed=tests/test-manifest.xml");
        // 只编译资源，由 lib.rs 的 cfg(test) 链接；不向桌面重复嵌入 manifest。
        embed_resource::compile_for(
            "tests/test-manifest.rc",
            std::iter::empty::<&str>(),
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("test manifest compilation failed");
        println!(
            "cargo:rustc-link-search=native={}",
            std::env::var("OUT_DIR").unwrap()
        );
    }
    tauri_build::build()
}
