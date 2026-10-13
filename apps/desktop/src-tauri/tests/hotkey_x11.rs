//! Linux hotkey regressions against the dependency used by the native Tauri plugin.
//!
//! Positive tests require an explicitly owned Xvfb display; they never connect to an
//! ordinary user's display during the workspace suite. Run with:
//! xvfb-run -a env HOLDMAP_X11_TEST=1 cargo test -p holdmap-desktop --locked \
//!   --test hotkey_x11 -- --include-ignored --test-threads=1
#![cfg(target_os = "linux")]

use global_hotkey::hotkey::{Code, Modifiers};
use global_hotkey::{hotkey::HotKey, Error, GlobalHotKeyManager};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, Keycode, ModMask, Window};
use x11rb::protocol::ErrorKind;
use x11rb::rust_connection::RustConnection;

fn locks() -> [ModMask; 4] {
    [
        ModMask::default(),
        ModMask::M2,
        ModMask::LOCK,
        ModMask::M2 | ModMask::LOCK,
    ]
}

struct OwnedChild(Option<Child>);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn run_owned_case(name: &str, configure: impl FnOnce(&mut Command), expected: &str) {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args(["--ignored", "--exact", name, "--nocapture"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut command);
    let mut owned = OwnedChild(Some(command.spawn().expect("owned X11 regression child")));
    let deadline = Instant::now() + Duration::from_secs(10);
    while owned
        .0
        .as_mut()
        .unwrap()
        .try_wait()
        .expect("child status")
        .is_none()
    {
        assert!(Instant::now() < deadline, "X11 case {name} hung");
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = owned
        .0
        .take()
        .unwrap()
        .wait_with_output()
        .expect("child output");
    assert!(
        output.status.success(),
        "owned child failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
}

#[test]
fn unavailable_x11_never_reports_registration_or_removal_success() {
    run_owned_case(
        "unavailable_x11_child",
        |command| {
            command
                .env("HOLDMAP_X11_UNAVAILABLE_CHILD", "1")
                // This invalid syntax fails locally before any DNS/socket operation.
                .env("DISPLAY", "holdmap-invalid-display");
        },
        "all four unavailable-X11 operations refused",
    );
}

#[test]
#[ignore = "finite subprocess fixture; harmless when not armed by its parent"]
fn unavailable_x11_child() {
    if std::env::var("HOLDMAP_X11_UNAVAILABLE_CHILD").as_deref() != Ok("1") {
        return;
    }
    assert_eq!(
        std::env::var("DISPLAY").as_deref(),
        Ok("holdmap-invalid-display")
    );
    // Initialization remains asynchronous so an optional unavailable shortcut cannot
    // abort Tauri plugin setup. Each actual operation must reject the failed worker.
    let manager = GlobalHotKeyManager::new().expect("optional manager setup");
    let key = HotKey::new(Some(Modifiers::ALT), Code::F9);
    for result in [
        manager.register(key),
        manager.unregister(key),
        manager.register_all(&[key]),
        manager.unregister_all(&[key]),
    ] {
        assert!(matches!(result, Err(Error::OsError(error))
            if error.kind() == std::io::ErrorKind::BrokenPipe));
    }
    println!("all four unavailable-X11 operations refused");
}

fn private_display(case: &str) -> Option<(RustConnection, Window)> {
    if std::env::var("HOLDMAP_X11_TEST").as_deref() != Ok("1") {
        return None;
    }
    if std::env::var("HOLDMAP_X11_CASE_CHILD").as_deref() != Ok(case) {
        run_owned_case(
            case,
            |command| {
                command.env("HOLDMAP_X11_CASE_CHILD", case);
            },
            "test result: ok.",
        );
        return None;
    }
    let (connection, screen) = RustConnection::connect(None).expect("owned Xvfb connection");
    let root = connection.setup().roots[screen].root;
    Some((connection, root))
}

fn keycode(connection: &RustConnection, keysym: u32) -> Keycode {
    let setup = connection.setup();
    let mapping = connection
        .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
        .unwrap()
        .reply()
        .unwrap();
    let index = mapping
        .keysyms
        .chunks(mapping.keysyms_per_keycode as usize)
        .position(|symbols| symbols.contains(&keysym))
        .expect("fixture keysym mapped");
    setup.min_keycode + u8::try_from(index).unwrap()
}

fn grab(
    connection: &RustConnection,
    root: Window,
    code: Keycode,
    lock: ModMask,
) -> Result<(), ReplyError> {
    connection
        .grab_key(
            false,
            root,
            ModMask::M1 | lock,
            code,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
}

fn assert_free(connection: &RustConnection, root: Window, code: Keycode, lock: ModMask) {
    grab(connection, root, code, lock).expect("independent client must acquire released shortcut");
    connection
        .ungrab_key(code, root, ModMask::M1 | lock)
        .unwrap()
        .check()
        .unwrap();
}

fn assert_reserved(connection: &RustConnection, root: Window, code: Keycode, lock: ModMask) {
    assert!(matches!(grab(connection, root, code, lock),
        Err(ReplyError::X11Error(error)) if error.error_kind == ErrorKind::Access));
}

#[test]
#[ignore = "requires an explicitly owned Xvfb display and HOLDMAP_X11_TEST=1"]
fn unregister_releases_every_caps_and_num_lock_variant_before_returning() {
    let Some((probe, root)) =
        private_display("unregister_releases_every_caps_and_num_lock_variant_before_returning")
    else {
        return;
    };
    let manager = GlobalHotKeyManager::new().unwrap();
    let key = HotKey::new(Some(Modifiers::ALT), Code::F9);
    let code = keycode(&probe, 0xffc6); // F9
    for lock in locks() {
        assert_free(&probe, root, code, lock);
    }
    manager.register(key).unwrap();
    for lock in locks() {
        assert_reserved(&probe, root, code, lock);
    }
    manager.unregister(key).unwrap();
    // No further request to the manager can flush a previously buffered ungrab.
    for lock in locks() {
        assert_free(&probe, root, code, lock);
    }
}

#[test]
#[ignore = "requires an explicitly owned Xvfb display and HOLDMAP_X11_TEST=1"]
fn unregister_all_releases_every_registered_key_and_lock_variant() {
    let Some((probe, root)) =
        private_display("unregister_all_releases_every_registered_key_and_lock_variant")
    else {
        return;
    };
    let manager = GlobalHotKeyManager::new().unwrap();
    let keys = [
        HotKey::new(Some(Modifiers::ALT), Code::F10),
        HotKey::new(Some(Modifiers::ALT), Code::F11),
    ];
    let codes = [keycode(&probe, 0xffc7), keycode(&probe, 0xffc8)];
    manager.register_all(&keys).unwrap();
    for code in codes {
        for lock in locks() {
            assert_reserved(&probe, root, code, lock);
        }
    }
    manager.unregister_all(&keys).unwrap();
    for code in codes {
        for lock in locks() {
            assert_free(&probe, root, code, lock);
        }
    }
}

#[test]
#[ignore = "requires an explicitly owned Xvfb display and HOLDMAP_X11_TEST=1"]
fn partial_modifier_collision_rolls_back_only_the_failed_managers_grabs() {
    let Some((owner, root)) =
        private_display("partial_modifier_collision_rolls_back_only_the_failed_managers_grabs")
    else {
        return;
    };
    let (probe, _) = RustConnection::connect(None).unwrap();
    let manager = GlobalHotKeyManager::new().unwrap();
    let key = HotKey::new(Some(Modifiers::ALT), Code::F12);
    let code = keycode(&owner, 0xffc9);
    // The driver has already grabbed plain and NumLock when CapsLock collides.
    grab(&owner, root, code, ModMask::LOCK).unwrap();
    assert!(
        matches!(manager.register(key), Err(Error::AlreadyRegistered(actual)) if actual == key)
    );
    for lock in [ModMask::default(), ModMask::M2, ModMask::M2 | ModMask::LOCK] {
        assert_free(&probe, root, code, lock);
    }
    assert_reserved(&probe, root, code, ModMask::LOCK);
    owner
        .ungrab_key(code, root, ModMask::M1 | ModMask::LOCK)
        .unwrap()
        .check()
        .unwrap();
    manager.register(key).unwrap();
    manager.unregister(key).unwrap();
    for lock in locks() {
        assert_free(&probe, root, code, lock);
    }
}

#[test]
#[ignore = "requires an explicitly owned Xvfb display and HOLDMAP_X11_TEST=1"]
fn failed_batch_stops_before_later_keys_and_returns_the_collision() {
    let Some((owner, root)) =
        private_display("failed_batch_stops_before_later_keys_and_returns_the_collision")
    else {
        return;
    };
    let (probe, _) = RustConnection::connect(None).unwrap();
    let manager = GlobalHotKeyManager::new().unwrap();
    let keys = [Code::F5, Code::F6, Code::F7].map(|key| HotKey::new(Some(Modifiers::ALT), key));
    let collision = keycode(&owner, 0xffc3); // F6
    let later = keycode(&owner, 0xffc4); // F7
    grab(&owner, root, collision, ModMask::LOCK).unwrap();
    assert!(
        matches!(manager.register_all(&keys), Err(Error::AlreadyRegistered(actual)) if actual == keys[1])
    );
    // Synchronize with the worker after the failed batch. An intermediate error reply
    // must not allow the worker to continue registering later keys in that batch.
    manager.unregister(keys[0]).unwrap();
    for lock in locks() {
        assert_free(&probe, root, later, lock);
    }
    for lock in [ModMask::default(), ModMask::M2, ModMask::M2 | ModMask::LOCK] {
        assert_free(&probe, root, collision, lock);
    }
    assert_reserved(&probe, root, collision, ModMask::LOCK);
}
