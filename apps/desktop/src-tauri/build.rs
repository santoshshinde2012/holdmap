fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_screen_capture();
    }
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

/// Keep the public ScreenCaptureKit adapter inside the signed executable. The
/// framework is weak linked so the ordinary app still launches on macOS 11–13.
fn build_screen_capture() {
    use std::{path::PathBuf, process::Command};

    fn output(command: &mut Command) -> String {
        let result = command
            .output()
            .expect("couldn't run the macOS build tools");
        assert!(
            result.status.success(),
            "macOS screen capture build failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout)
            .expect("macOS build tools returned invalid UTF-8")
            .trim()
            .to_owned()
    }

    println!("cargo:rerun-if-changed=native/screen_capture.swift");
    for variable in ["DEVELOPER_DIR", "SDKROOT"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("unsupported macOS screen capture architecture"),
    };
    let target = format!("{arch}-apple-macosx11.0");
    let sdk = std::env::var("SDKROOT").unwrap_or_else(|_| {
        output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]))
    });
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("missing Cargo output directory"));
    let object = out.join("screen_capture.o");
    output(
        Command::new("xcrun")
            .args([
                "swiftc",
                "-parse-as-library",
                "-emit-object",
                "-O",
                "-swift-version",
                "5",
                "-module-name",
                "HoldmapScreenCapture",
                "-target",
                &target,
                "-sdk",
                &sdk,
                "-Xfrontend",
                "-disable-autolink-framework",
                "-Xfrontend",
                "ScreenCaptureKit",
                "native/screen_capture.swift",
                "-o",
            ])
            .arg(&object),
    );
    output(
        Command::new("xcrun")
            .arg("ar")
            .arg("crs")
            .arg(out.join("libholdmap_screen_capture.a"))
            .arg(object),
    );
    let info: serde_json::Value = serde_json::from_str(&output(Command::new("xcrun").args([
        "swiftc",
        "-print-target-info",
        "-target",
        &target,
    ])))
    .expect("invalid Swift target information");
    for directory in info["paths"]["runtimeLibraryPaths"]
        .as_array()
        .expect("missing Swift runtime library paths")
    {
        println!(
            "cargo:rustc-link-search=native={}",
            directory.as_str().expect("invalid Swift runtime path")
        );
    }
    // Compatibility archives are selected by the object's Swift autolink entries.
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,ScreenCaptureKit");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
