//! Exercise the production browser encoder in an owned macOS WKWebView.
//! The runner supplies synthetic canvas assets; no real screen or native chooser is opened.

#[cfg(target_os = "macos")]
#[path = "../src/local_window.rs"]
mod local_window;

#[cfg(target_os = "macos")]
mod fixture {
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::borrow::Cow;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tauri::ipc::InvokeBody;
    use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
    use tauri::{AppHandle, Assets, Runtime, State, WebviewWindow};

    #[derive(Deserialize)]
    struct Fixture {
        root: PathBuf,
        app: PathBuf,
        assets: PathBuf,
        identifier: String,
    }

    struct FixtureAssets(BTreeMap<AssetKey, Vec<u8>>);

    impl<R: Runtime> Assets<R> for FixtureAssets {
        fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
            self.0.get(key).map(|bytes| Cow::Borrowed(bytes.as_slice()))
        }

        fn iter(&self) -> Box<AssetsIter<'_>> {
            Box::new(
                self.0.iter().map(|(key, bytes)| {
                    (Cow::Borrowed(key.as_ref()), Cow::Borrowed(bytes.as_slice()))
                }),
            )
        }

        fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn load_assets(
        root: &Path,
        directory: &Path,
        assets: &mut FixtureAssets,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(std::io::Error::other("fixture asset must not be a link"));
            }
            if kind.is_dir() {
                load_assets(root, &entry.path(), assets)?;
            } else if kind.is_file() {
                if entry.metadata()?.len() > 16 * 1024 * 1024 || assets.0.len() >= 128 {
                    return Err(std::io::Error::other("fixture assets exceed their limits"));
                }
                let path = entry.path();
                let relative = path.strip_prefix(root).map_err(std::io::Error::other)?;
                assets.0.insert(relative.into(), std::fs::read(&path)?);
            }
        }
        Ok(())
    }

    struct FixtureState {
        root: PathBuf,
        files: Mutex<BTreeMap<String, serde_json::Value>>,
    }

    fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    // The production frontend save adapter reaches this real Wry binary IPC endpoint.
    // Only this fixture replaces the interactive chooser with a private fixed destination.
    #[tauri::command]
    async fn save_screen_recording(
        window: WebviewWindow,
        state: State<'_, FixtureState>,
        request: tauri::ipc::Request<'_>,
    ) -> Result<Option<String>, String> {
        super::local_window::authorize(&window, "Fixture recording save")?;
        let filename = request
            .headers()
            .get("x-holdmap-recording-name")
            .and_then(|value| value.to_str().ok())
            .ok_or("missing recording name")?;
        if !matches!(filename, "holdmap-runtime.mp4" | "holdmap-runtime.gif") {
            return Err("only owned fixture filenames are permitted".into());
        }
        let InvokeBody::Raw(bytes) = request.body() else {
            return Err("recording did not arrive as raw binary IPC".into());
        };
        if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
            return Err("fixture recording exceeds its size limit".into());
        }
        let path = state.root.join(filename);
        write_private(&path, bytes).map_err(|error| error.to_string())?;
        if std::fs::read(&path).map_err(|error| error.to_string())? != *bytes {
            return Err("saved recording bytes differ from the IPC payload".into());
        }
        state.files.lock().unwrap().insert(
            filename.to_owned(),
            serde_json::json!({
                "bytes": bytes.len(),
                "sha256": format!("{:x}", Sha256::digest(bytes)),
            }),
        );
        Ok(Some(path.to_string_lossy().into_owned()))
    }

    #[tauri::command]
    fn finish_fixture(
        window: WebviewWindow,
        app: AppHandle,
        state: State<'_, FixtureState>,
        report: serde_json::Value,
    ) -> Result<(), String> {
        super::local_window::authorize(&window, "Fixture result")?;
        let result = serde_json::to_vec(&serde_json::json!({
            "browser": report,
            "native_files": *state.files.lock().unwrap(),
        }))
        .map_err(|error| error.to_string())?;
        if result.len() > 64 * 1024 {
            return Err("fixture result exceeds its limit".into());
        }
        write_private(&state.root.join("result.json"), &result)
            .map_err(|error| error.to_string())?;
        app.exit(0);
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        let [flag, path] = arguments.as_slice() else {
            return Err("use scripts/check-screen-recording-runtime.py".into());
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
        let app = fixture.app.canonicalize()?;
        let assets_root = fixture.assets.canonicalize()?;
        if !root.starts_with(std::env::temp_dir().canonicalize()?)
            || !root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("holdmap-screen-runtime-")
            })
            || root.metadata()?.permissions().mode() & 0o077 != 0
            || !app.starts_with(&root)
            || !assets_root.starts_with(&root)
            || std::env::current_exe()?.canonicalize()?
                != app.join("Contents/MacOS/screen_recording_runtime")
            || std::fs::read_to_string(root.join("owned-fixture"))?
                != "holdmap synthetic screen fixture"
            || !fixture
                .identifier
                .strip_prefix("dev.holdmap.recordingfixture.")
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                })
        {
            return Err("refusing to operate outside the runner's private scratch app".into());
        }
        holdmap_core::util::create_private_dir(&root)?;
        let mut assets = FixtureAssets(BTreeMap::new());
        load_assets(&assets_root, &assets_root, &mut assets)?;
        let mut context = tauri::generate_context!();
        context.config_mut().app.windows.clear();
        context.config_mut().build.dev_url = None;
        context.config_mut().identifier = fixture.identifier;
        context.set_assets(Box::new(assets));
        tauri::Builder::default()
            .manage(FixtureState {
                root,
                files: Mutex::new(BTreeMap::new()),
            })
            .invoke_handler(super::local_window::guard(tauri::generate_handler![
                save_screen_recording,
                finish_fixture
            ]))
            .setup(|app| {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("Owned Holdmap synthetic recording check")
                .inner_size(480.0, 300.0)
                .focused(false)
                .incognito(true)
                .build()?;
                Ok(())
            })
            .run(context)?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    fixture::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("The owned synthetic recording runtime check currently targets macOS.");
}
