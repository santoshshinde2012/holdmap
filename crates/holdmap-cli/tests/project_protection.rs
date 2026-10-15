//! Exercise project protection through the shipped CLI using only owned, finite fixtures.

use assert_cmd::Command;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const FIXTURE_TIMEOUT: Duration = Duration::from_secs(90);
// Cold parallel process launches can exceed five seconds on macOS. Keep readiness
// bounded without retrying policy work; diagnostics distinguish startup stages.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize, Deserialize)]
struct Ready {
    pid: u32,
    ports: Vec<u16>,
    child: Option<Box<Ready>>,
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Closing the pipe also releases a fixture's children if its parent was killed.
        drop(self.0.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_ready(path: &Path, child: &mut Child) -> Ready {
    let started = Instant::now();
    let deadline = started + READY_TIMEOUT;
    loop {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(ready) = serde_json::from_slice::<Ready>(&bytes) {
                assert_eq!(ready.pid, child.id());
                if started.elapsed() >= Duration::from_secs(5) {
                    eprintln!(
                        "owned fixture PID {} became ready after {:?}",
                        child.id(),
                        started.elapsed()
                    );
                }
                return ready;
            }
        }
        let status = child.try_wait().unwrap();
        assert!(
            status.is_none() && Instant::now() < deadline,
            "owned fixture PID {} did not become ready after {:?}; status: {status:?}; {}",
            child.id(),
            started.elapsed(),
            fixture_diagnostics(path)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn fixture_diagnostics(path: &Path) -> String {
    ["stage", "stdout", "stderr"]
        .into_iter()
        .map(|extension| {
            let mut bytes = Vec::new();
            if let Ok(file) = std::fs::File::open(path.with_extension(extension)) {
                let _ = file.take(8192).read_to_end(&mut bytes);
            }
            format!("{extension}: {:?}", String::from_utf8_lossy(&bytes))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn fixture_command(exe: &Path, cwd: &Path, ready: &Path, count: usize) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    command
        .args([
            "--ignored",
            "--exact",
            "vite_listener_fixture",
            "--nocapture",
        ])
        .env("HOLDMAP_POLICY_READY", ready)
        .env("HOLDMAP_POLICY_PORT_COUNT", count.to_string())
        .env_remove("HOLDMAP_POLICY_CHILD")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(holdmap_core::util::create_private(&ready.with_extension("stdout")).unwrap())
        .stderr(holdmap_core::util::create_private(&ready.with_extension("stderr")).unwrap());
    command
}

struct Listener {
    child: OwnedChild,
    ready: Ready,
    // Keep the executable alias alive until the owned child has been cleaned up.
    #[cfg(unix)]
    _executable_directory: tempfile::TempDir,
}

impl Listener {
    fn start(directory: &Path, count: usize, tree: bool) -> Self {
        std::fs::create_dir_all(directory).unwrap();
        // A separate name prevents --name from also matching the test runner (an ancestor
        // that the core policy independently protects). The executable is still this test.
        let suffix = directory.file_name().unwrap().to_string_lossy();
        let name = format!(
            "hm-policy-{}-{suffix}{}",
            std::process::id(),
            std::env::consts::EXE_SUFFIX
        );
        let current_exe = std::env::current_exe().unwrap();
        #[cfg(unix)]
        let executable_directory = tempfile::Builder::new()
            .prefix("hm-policy-bin-")
            .tempdir_in(current_exe.parent().unwrap())
            .unwrap();
        #[cfg(unix)]
        let exe = {
            let exe = executable_directory.path().join(name);
            #[cfg(target_os = "macos")]
            {
                use std::os::unix::fs::MetadataExt;
                // macOS policy can reject a nonexistent mixed path for renamed hard
                // links before main: https://github.com/rust-lang/cargo/issues/10060.
                // The private copy has its own inode and is closed before any launch.
                std::fs::copy(&current_exe, &exe).unwrap();
                let source = std::fs::metadata(&current_exe).unwrap();
                let alias = std::fs::metadata(&exe).unwrap();
                assert_ne!(
                    (source.dev(), source.ino()),
                    (alias.dev(), alias.ino()),
                    "macOS executable aliases must not share the source inode"
                );
            }
            #[cfg(not(target_os = "macos"))]
            // A parallel fork can retain a copied executable's writable descriptor until
            // exec, causing Linux ETXTBSY even after copy() closes its own descriptor.
            // Linking the running binary on its own filesystem creates no writable handle.
            // Its working directory stays in the isolated project, outside checkout policy.
            std::fs::hard_link(&current_exe, &exe).unwrap();
            exe
        };
        #[cfg(not(unix))]
        let exe = {
            let exe = directory.join(name);
            std::fs::copy(&current_exe, &exe).unwrap();
            exe
        };
        let ready_file = directory.join("ready.json");
        let mut command = fixture_command(&exe, directory, &ready_file, count);
        if tree {
            command.env("HOLDMAP_POLICY_CHILD", "1");
        }
        let mut child = OwnedChild(command.spawn().unwrap());
        let ready = wait_ready(&ready_file, &mut child.0);
        let listener = Self {
            child,
            ready,
            #[cfg(unix)]
            _executable_directory: executable_directory,
        };
        listener.assert_alive();
        listener
    }

    fn assert_alive(&self) {
        fn check(ready: &Ready) {
            for port in &ready.ports {
                TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], *port)),
                    Duration::from_secs(1),
                )
                .unwrap_or_else(|error| {
                    panic!("owned PID {} lost port {port}: {error}", ready.pid)
                });
            }
            if let Some(child) = &ready.child {
                check(child);
            }
        }
        assert_eq!(self.child.0.id(), self.ready.pid);
        check(&self.ready);
    }
}

struct Project {
    directory: tempfile::TempDir,
    root: PathBuf,
    cwd: PathBuf,
    marker: PathBuf,
}

impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let cwd = root.join("nested/subdir");
        std::fs::create_dir_all(&cwd).unwrap();
        let marker = directory.path().join("executed.marker");
        Self {
            directory,
            root,
            cwd,
            marker,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("holdmap").unwrap();
        command
            .env("HOLDMAP_HOME", self.directory.path().join("state"))
            .env("HOLDMAP_COLOR", "never")
            .env("HOLDMAP_POLICY_MARKER", &self.marker)
            .current_dir(&self.cwd)
            .arg("--no-docker")
            .timeout(Duration::from_secs(15));
        command
    }

    fn protect(&self, ports: &[u16]) {
        write_config(&self.root.join(".holdmap.toml"), ports, None);
    }

    fn outside_listener(&self, count: usize) -> Listener {
        Listener::start(&self.directory.path().join("outside"), count, false)
    }

    fn assert_no_command(&self) {
        assert!(
            !self.marker.exists(),
            "a refused action executed its command"
        );
        assert!(
            !self.directory.path().join("state/history.jsonl").exists(),
            "a refused action recorded a stop"
        );
    }
}

fn marker_args() -> Vec<String> {
    vec![
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        "--ignored".into(),
        "--exact".into(),
        "write_marker_fixture".into(),
        "--nocapture".into(),
    ]
}

fn marker_line() -> String {
    let args = marker_args();
    if cfg!(windows) {
        // This is cmd syntax; shell_command supplies cmd /S /C's outer quotes.
        format!("\"{}\" {}", args[0], args[1..].join(" "))
    } else {
        args.iter()
            .map(|arg| format!("'{}'", arg.replace('\'', "'\"'\"'")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn write_config(path: &Path, ports: &[u16], service: Option<(u16, Option<&Path>)>) {
    let mut text = format!("protect = {ports:?}\n");
    if let Some((port, cwd)) = service {
        let command = marker_line();
        text.push_str(&format!(
            "[services.web]\nport = {port}\ncommand = {}\n",
            serde_json::to_string(&command).unwrap()
        ));
        if let Some(cwd) = cwd {
            text.push_str(&format!(
                "cwd = {}\n",
                serde_json::to_string(&cwd.to_string_lossy()).unwrap()
            ));
        }
    }
    std::fs::write(path, text).unwrap();
}

fn blocked_json(output: Output, ports: &[u16]) {
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let results: Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = results.as_array().unwrap();
    assert_eq!(results.len(), 1);
    let plan = &results[0]["plan"];
    assert_eq!(plan["blocked"]["kind"], "protected");
    assert_eq!(plan["steps"], serde_json::json!([]));
    assert!(results[0]["report"].is_null());
    let message = plan["blocked"]["message"].as_str().unwrap();
    assert!(message.contains("project `protect`"), "{message}");
    for port in ports {
        assert!(message.contains(&port.to_string()), "{message}");
    }
}

fn assert_refused(output: Output, code: i32) -> String {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("protect"), "{text}");
    text
}

#[test]
fn stop_target_variants_and_all_dev_have_non_executable_protected_plans() {
    let project = Project::new();
    let listener = project.outside_listener(1);
    let port = listener.ready.ports[0].to_string();
    let pid = listener.ready.pid.to_string();
    let output = project
        .command()
        .args(["list", &format!("pid:{pid}"), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
    let row = snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["port"] == listener.ready.ports[0])
        .unwrap();
    assert_eq!(row["is_dev"], true, "fixture must participate in --all-dev");
    let name = row["process"]["name"].as_str().unwrap();
    let baseline = project
        .command()
        .args(["stop", &port, "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(
        baseline.status.success(),
        "fixture must be independently stoppable: {baseline:?}"
    );
    project.protect(&listener.ready.ports);
    for targets in [
        vec![port.as_str()],
        vec!["--pid", &pid],
        vec!["--name", name],
        vec!["--all-dev"],
    ] {
        let output = project
            .command()
            .arg("stop")
            .args(targets)
            .args(["--dry-run", "--json"])
            .output()
            .unwrap();
        blocked_json(output, &listener.ready.ports);
        listener.assert_alive();
    }
    let output = project
        .command()
        .args(["stop", &format!("pid:{pid}"), "--dry-run", "--json"])
        .output()
        .unwrap();
    blocked_json(output, &listener.ready.ports);
    // Only the documented explicit stop override permits this otherwise safe fixture plan.
    let output = project
        .command()
        .args(["stop", &port, "--allow-protected", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value[0]["plan"]["blocked"].is_null());
    assert!(!value[0]["plan"]["steps"].as_array().unwrap().is_empty());
    listener.assert_alive();
    project.assert_no_command();
}

#[test]
fn actual_numeric_and_pid_stops_preserve_every_port_of_the_protected_owner() {
    let project = Project::new();
    let listener = project.outside_listener(2);
    project.protect(&[listener.ready.ports[1]]);
    let first = listener.ready.ports[0].to_string();
    let protected = listener.ready.ports[1].to_string();
    let pid = listener.ready.pid.to_string();
    for target in [
        vec![protected.as_str()],
        vec![first.as_str()],
        vec!["--pid", &pid],
    ] {
        blocked_json(
            project
                .command()
                .arg("stop")
                .args(target)
                .args(["--yes", "--json", "--timeout", "100ms"])
                .output()
                .unwrap(),
            &[listener.ready.ports[1]],
        );
        listener.assert_alive();
    }
    blocked_json(
        project
            .command()
            .args([
                "stop",
                &format!("pid:{pid}"),
                "--no-tree",
                "--yes",
                "--json",
            ])
            .output()
            .unwrap(),
        &[listener.ready.ports[1]],
    );
    listener.assert_alive();
    project.assert_no_command();
}

#[cfg(unix)]
#[test]
fn stopping_a_parent_even_without_tree_cannot_close_a_protected_child_listener() {
    let project = Project::new();
    let listener = Listener::start(&project.directory.path().join("tree"), 1, true);
    let child = listener.ready.child.as_ref().unwrap();
    project.protect(&child.ports);
    let pid = listener.ready.pid.to_string();
    for extra in [None, Some("--no-tree")] {
        let mut command = project.command();
        command.args(["stop", "--pid", &pid, "--yes", "--json"]);
        if let Some(extra) = extra {
            command.arg(extra);
        }
        blocked_json(command.output().unwrap(), &child.ports);
        listener.assert_alive();
    }
    project.assert_no_command();
}

#[test]
fn run_and_busy_restart_refuse_to_replace_an_owner_with_another_protected_port() {
    let project = Project::new();
    let listener = project.outside_listener(2);
    project.protect(&[listener.ready.ports[1]]);
    let first = listener.ready.ports[0].to_string();
    let output = project
        .command()
        .args(["run", "--port", &first, "--yes", "--timeout", "100ms", "--"])
        .args(marker_args())
        .output()
        .unwrap();
    assert_refused(output, 3);
    listener.assert_alive();
    project.assert_no_command();
    let output = project
        .command()
        .args(["restart", &first, "--yes", "--timeout", "100ms"])
        .output()
        .unwrap();
    assert_refused(output, 3);
    listener.assert_alive();
    let protected = listener.ready.ports[1].to_string();
    assert_refused(
        project
            .command()
            .args(["restart", &protected, "--yes"])
            .output()
            .unwrap(),
        3,
    );
    listener.assert_alive();
    project.assert_no_command();
}

#[test]
fn stack_down_and_up_replace_preserve_a_protected_sibling_port_without_running_commands() {
    let project = Project::new();
    let listener = Listener::start(&project.root.join("listener"), 2, false);
    write_config(
        &project.root.join(".holdmap.toml"),
        &[listener.ready.ports[1]],
        Some((listener.ready.ports[0], None)),
    );
    assert_refused(
        project
            .command()
            .args(["down", "--yes", "--timeout", "100ms"])
            .output()
            .unwrap(),
        3,
    );
    listener.assert_alive();
    project.assert_no_command();

    let outside = project.outside_listener(2);
    write_config(
        &project.root.join(".holdmap.toml"),
        &[outside.ready.ports[1]],
        Some((outside.ready.ports[0], None)),
    );
    assert_refused(
        project
            .command()
            .args(["up", "--replace", "--yes"])
            .output()
            .unwrap(),
        3,
    );
    outside.assert_alive();
    listener.assert_alive();
    project.assert_no_command();
}

#[test]
fn explicit_stack_file_unions_its_protection_with_the_nearest_ancestor() {
    let project = Project::new();
    let listener = project.outside_listener(3);
    project.protect(&[listener.ready.ports[1]]);
    let explicit_root = project.directory.path().join("explicit");
    std::fs::create_dir(&explicit_root).unwrap();
    let explicit = explicit_root.join("stack.toml");
    write_config(
        &explicit,
        &[listener.ready.ports[2]],
        Some((listener.ready.ports[0], None)),
    );
    let text = assert_refused(
        project
            .command()
            .args(["up", "--replace", "--yes", "--file"])
            .arg(&explicit)
            .output()
            .unwrap(),
        3,
    );
    for port in &listener.ready.ports[1..] {
        assert!(text.contains(&port.to_string()), "{text}");
    }
    listener.assert_alive();
    // Use the OS-reported path so aliases such as macOS /var -> /private/var and
    // Windows extended path prefixes do not turn this owned service into a conflict.
    let output = project
        .command()
        .args(["list", &format!("pid:{}", listener.ready.pid), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entry = snapshot["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["port"] == listener.ready.ports[0])
        .unwrap();
    let cwd = PathBuf::from(
        entry["process"]["cwd"]
            .as_str()
            .expect("owned fixture cwd is visible"),
    );
    write_config(
        &explicit,
        &[listener.ready.ports[2]],
        Some((listener.ready.ports[0], Some(&cwd))),
    );
    let text = assert_refused(
        project
            .command()
            .args(["down", "--yes", "--file"])
            .arg(&explicit)
            .output()
            .unwrap(),
        3,
    );
    for port in &listener.ready.ports[1..] {
        assert!(text.contains(&port.to_string()), "{text}");
    }
    listener.assert_alive();
    project.assert_no_command();
}

#[test]
fn legacy_ancestor_protection_is_honored_from_a_nested_working_directory() {
    let project = Project::new();
    let listener = project.outside_listener(1);
    write_config(
        &project.root.join(".portwise.toml"),
        &listener.ready.ports,
        None,
    );
    blocked_json(
        project
            .command()
            .args([
                "stop",
                &listener.ready.ports[0].to_string(),
                "--dry-run",
                "--json",
            ])
            .output()
            .unwrap(),
        &listener.ready.ports,
    );
    listener.assert_alive();
}

fn assert_invalid_nearest_preserves_fixture(project: &Project, listener: &Listener) {
    let invalid = |output: Output| {
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(".holdmap.toml"),
            "{output:?}"
        );
    };
    let port = listener.ready.ports[0].to_string();
    let pid = listener.ready.pid.to_string();
    for args in [
        vec!["stop", port.as_str(), "--yes"],
        vec!["stop", "--pid", &pid, "--yes", "--allow-protected"],
        vec!["restart", port.as_str(), "--yes"],
        vec!["down", "--yes"],
    ] {
        invalid(project.command().args(args).output().unwrap());
        listener.assert_alive();
    }
    invalid(
        project
            .command()
            .args(["run", "--port", &port, "--yes", "--"])
            .args(marker_args())
            .output()
            .unwrap(),
    );
    invalid(
        project
            .command()
            .args(["up", "--replace", "--yes", "--file"])
            .arg(project.root.join(".holdmap.toml"))
            .output()
            .unwrap(),
    );
    listener.assert_alive();
    project.assert_no_command();
}

#[test]
fn malformed_and_nonregular_nearest_files_fail_closed_without_falling_back_to_an_ancestor() {
    let project = Project::new();
    let listener = project.outside_listener(1);
    write_config(
        &project.root.join(".holdmap.toml"),
        &[],
        Some((listener.ready.ports[0], None)),
    );
    let nearest = project.cwd.join(".holdmap.toml");
    std::fs::write(&nearest, "protect = [this is not valid TOML").unwrap();
    assert_invalid_nearest_preserves_fixture(&project, &listener);
    std::fs::remove_file(&nearest).unwrap();
    std::fs::create_dir(&nearest).unwrap();
    assert_invalid_nearest_preserves_fixture(&project, &listener);
}

#[cfg(unix)]
#[test]
fn writable_and_dangling_nearest_files_fail_closed_without_any_override() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let project = Project::new();
    let listener = project.outside_listener(1);
    write_config(
        &project.root.join(".holdmap.toml"),
        &[],
        Some((listener.ready.ports[0], None)),
    );
    let nearest = project.cwd.join(".holdmap.toml");
    write_config(&nearest, &[], None);
    std::fs::set_permissions(&nearest, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_invalid_nearest_preserves_fixture(&project, &listener);
    std::fs::remove_file(&nearest).unwrap();
    symlink(project.cwd.join("missing.toml"), &nearest).unwrap();
    assert_invalid_nearest_preserves_fixture(&project, &listener);
}

#[test]
fn the_stack_command_fixture_can_write_its_scoped_marker() {
    let project = Project::new();
    let mut command = Command::from_std(holdmap_core::util::shell_command(&marker_line()));
    command
        .env("HOLDMAP_POLICY_MARKER", &project.marker)
        .timeout(Duration::from_secs(15));
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(&project.marker).unwrap(),
        "executed"
    );
}

#[test]
#[ignore = "internal finite subprocess listener fixture"]
fn vite_listener_fixture() {
    let ready_file = PathBuf::from(
        std::env::var_os("HOLDMAP_POLICY_READY").expect("parent supplied readiness path"),
    );
    std::fs::write(ready_file.with_extension("stage"), "fixture entered").unwrap();
    let count: usize = std::env::var("HOLDMAP_POLICY_PORT_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&count));
    let listeners: Vec<TcpListener> = (0..count)
        .map(|_| TcpListener::bind(("127.0.0.1", 0)).unwrap())
        .collect();
    for listener in &listeners {
        listener.set_nonblocking(true).unwrap();
    }
    std::fs::write(ready_file.with_extension("stage"), "listeners bound").unwrap();
    let mut child = None;
    let child_ready = if std::env::var_os("HOLDMAP_POLICY_CHILD").is_some() {
        let child_file = ready_file.with_extension("child.json");
        let cwd = std::env::current_dir().unwrap();
        let mut owned = OwnedChild(
            fixture_command(&std::env::current_exe().unwrap(), &cwd, &child_file, 1)
                .spawn()
                .unwrap(),
        );
        let ready = wait_ready(&child_file, &mut owned.0);
        child = Some(owned);
        Some(Box::new(ready))
    } else {
        None
    };
    std::fs::write(ready_file.with_extension("stage"), "publishing readiness").unwrap();
    let ready = Ready {
        pid: std::process::id(),
        ports: listeners
            .iter()
            .map(|listener| listener.local_addr().unwrap().port())
            .collect(),
        child: child_ready,
    };
    std::fs::write(ready_file, serde_json::to_vec(&ready).unwrap()).unwrap();
    let closed = Arc::new(AtomicBool::new(false));
    let reader_closed = Arc::clone(&closed);
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut bytes = [0; 8];
        while input.read(&mut bytes).is_ok_and(|count| count > 0) {}
        reader_closed.store(true, Ordering::Release);
    });
    let deadline = Instant::now() + FIXTURE_TIMEOUT;
    while Instant::now() < deadline && !closed.load(Ordering::Acquire) {
        for listener in &listeners {
            for _ in 0..16 {
                if listener.accept().is_err() {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(child);
}

#[test]
#[ignore = "internal command execution marker fixture"]
fn write_marker_fixture() {
    std::fs::write(
        std::env::var_os("HOLDMAP_POLICY_MARKER").expect("parent supplied marker path"),
        "executed",
    )
    .unwrap();
}
