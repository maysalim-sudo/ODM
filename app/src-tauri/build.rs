fn main() {
    tauri_build::build();
    // tauri-build links the resource (with the Common Controls v6 manifest that tauri needs)
    // into the app only. Tests that start a mock Tauri app need it too, or Windows refuses to
    // load them (STATUS_ENTRYPOINT_NOT_FOUND).
    #[cfg(windows)]
    {
        let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
        let lib = std::path::Path::new(&out).join("resource.lib");
        println!("cargo:rustc-link-arg-tests={}", lib.display());
    }
}
