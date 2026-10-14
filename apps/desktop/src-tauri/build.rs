fn main() {
    let attributes = tauri_build::Attributes::new();
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let attributes = if windows_msvc {
        // Tauri's resource compiler links the app manifest only into binaries. The
        // lib test harness also imports Common Controls v6 (TaskDialogIndirect), so
        // let the linker embed the same manifest into every executable artifact.
        // Generic link args cover lib unit tests; rustc-link-arg-tests does not.
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("missing Cargo manifest directory"),
        )
        .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        attributes
    };
    tauri_build::try_build(attributes).expect("failed to build Holdmap desktop resources");
}
