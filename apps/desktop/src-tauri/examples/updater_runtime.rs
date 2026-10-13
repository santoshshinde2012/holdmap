//! Run the unchanged native update adapter inside an owned scratch macOS application.
//! Driven by scripts/check-updater-runtime.py; never invoke from an installed app.

#[cfg(target_os = "macos")]
mod state {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    // Only the fields consumed by update.rs are provided; no scans or user stores run.
    pub struct AppState {
        pub updater: AtomicBool,
        pub installing_update: Arc<AtomicBool>,
    }
}

#[cfg(target_os = "macos")]
#[path = "../src/update.rs"]
#[allow(dead_code)]
mod update;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use serde::Deserialize;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Manager;

    #[derive(Deserialize)]
    struct Fixture {
        root: PathBuf,
        app: PathBuf,
        endpoint: String,
        public_key: String,
        result: PathBuf,
    }

    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [flag, path] = arguments.as_slice() else {
        return Err("use the owned scratch runner, not the example directly".into());
    };
    if flag != "--fixture" {
        return Err("expected --fixture".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err("fixture metadata exceeds its limit".into());
    }
    let fixture: Fixture = serde_json::from_slice(&bytes)?;
    let root = fixture.root.canonicalize()?;
    let app_path = fixture.app.canonicalize()?;
    let executable = std::env::current_exe()?.canonicalize()?;
    if !root.starts_with(std::env::temp_dir().canonicalize()?)
        || !root.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .starts_with("holdmap-updater-runtime-")
        })
        || !app_path.starts_with(&root)
        || executable != app_path.join("Contents/MacOS/updater_runtime")
        || !fixture.result.starts_with(&root)
        || std::fs::read_to_string(root.join("owned-fixture"))? != "holdmap updater fixture"
    {
        return Err("refusing to update outside the runner's private scratch application".into());
    }
    if root.metadata()?.permissions().mode() & 0o077 != 0 {
        return Err("the scratch root must be private to its owner".into());
    }
    // Reuse the production owner check without touching any global directory.
    holdmap_core::util::create_private_dir(&root)?;
    let result_parent = fixture
        .result
        .parent()
        .ok_or("missing result directory")?
        .canonicalize()?;
    if !result_parent.starts_with(&root) {
        return Err("result directory escaped the private scratch root".into());
    }
    holdmap_core::util::create_private_dir(&result_parent)?;
    let result_path = result_parent.join(
        fixture
            .result
            .file_name()
            .ok_or("missing result filename")?,
    );
    if std::fs::symlink_metadata(&result_path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("result file must not be a symlink".into());
    }
    let url: tauri::Url = fixture.endpoint.parse()?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("only an owned loopback fixture endpoint is supported".into());
    }

    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = format!("dev.holdmap.updaterfixture.{}", std::process::id());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": fixture.public_key,
            "endpoints": [fixture.endpoint],
            "requireSignedVersion": true,
            // Only this test's local server uses HTTP. Production config remains HTTPS.
            "dangerousInsecureTransportProtocol": true,
        }),
    );
    assert!(update::configured(&context));
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state::AppState {
            updater: AtomicBool::new(true),
            installing_update: Arc::new(AtomicBool::new(false)),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // The successful adapter installs and genuinely restarts this scratch
                // process. Only rejected attempts return here and exit the event loop.
                let result = update::install_update(handle.clone()).await;
                let installing = handle
                    .state::<state::AppState>()
                    .installing_update
                    .load(Ordering::Acquire);
                let output = serde_json::json!({
                    "error": result.err(),
                    "installing": installing,
                });
                let written = holdmap_core::util::create_private(&result_path)
                    .and_then(|mut file| file.write_all(&serde_json::to_vec(&output).unwrap()));
                if written.is_err() {
                    handle.exit(2);
                } else {
                    handle.exit(0);
                }
            });
            Ok(())
        })
        .build(context)?
        .run(|_, _| {});
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("The owned scratch updater runtime proof currently targets macOS.");
}
