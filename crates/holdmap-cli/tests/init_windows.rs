//! Windows project init must capture literal native argv, then run its generated stack.
#![cfg(windows)]

use assert_cmd::Command;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Output, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize, Serialize)]
struct Ready {
    pid: u32,
    port: u16,
    argv: Vec<String>,
    cwd: PathBuf,
    inherited: String,
}

struct Project {
    directory: tempfile::TempDir,
    root: PathBuf,
    cwd: PathBuf,
    exe: PathBuf,
    ready: PathBuf,
    quit: PathBuf,
    child: Option<Child>,
}

impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory
            .path()
            .join("project space & %HOLDMAP_INIT_EXPAND% ! $ ' 雪");
        let cwd = root.join("apps/work space");
        let bin = root.join("bin space & %HOLDMAP_INIT_EXPAND%");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("fixture.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let ready = directory.path().join("ready.json");
        let quit = directory.path().join("quit");
        Self {
            directory,
            root,
            cwd,
            exe,
            ready,
            quit,
            child: None,
        }
    }

    fn prepare(&self, command: &mut std::process::Command) {
        command
            .current_dir(&self.cwd)
            .env("HOLDMAP_INIT_READY", &self.ready)
            .env("HOLDMAP_INIT_QUIT", &self.quit)
            .env("HOLDMAP_INIT_ENV", "literal inherited %PATH% & !")
            .env("HOLDMAP_INIT_EXPAND", "must not expand")
            .env_remove("PORT")
            .env_remove("HOLDMAP_INIT_EXIT_CODE");
    }

    fn start(&mut self, extra: &[String]) -> Ready {
        let mut command = std::process::Command::new(&self.exe);
        self.prepare(&mut command);
        command
            .args([
                "--ignored",
                "--exact",
                "vite_init_listener_fixture",
                "--nocapture",
                "--",
            ])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        self.child = Some(command.spawn().unwrap());
        let ready = self.wait_ready();
        assert_eq!(ready.pid, self.child.as_ref().unwrap().id());
        ready
    }

    fn stop_original(&mut self) {
        let mut child = self.child.take().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        std::fs::remove_file(&self.ready).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("holdmap"));
        self.prepare(&mut command);
        command
            .current_dir(&self.root)
            .env("HOLDMAP_HOME", self.directory.path().join("state"))
            .env("HOLDMAP_COLOR", "never")
            .arg("--no-docker");
        let mut command = Command::from_std(command);
        command.timeout(Duration::from_secs(15));
        command
    }

    fn init(&self, port: u16) -> holdmap_core::stack::Stack {
        let output = self.command().arg("init").output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let stack = holdmap_core::stack::load(&self.root.join(".holdmap.toml")).unwrap();
        assert_eq!(stack.services.len(), 1, "init must find the owned listener");
        if stack.services[0].port != Some(port) {
            let pid = self.child.as_ref().unwrap().id();
            let table = holdmap_core::process::ProcessTable::capture();
            let process = table.get(pid).cloned().map(|mut process| {
                process.cmdline = holdmap_core::redact::args(&process.cmdline);
                process
            });
            let canonical_root = std::fs::canonicalize(&self.root).unwrap();
            let mut ready: Ready =
                serde_json::from_slice(&std::fs::read(&self.ready).unwrap()).unwrap();
            ready.argv = holdmap_core::redact::args(&ready.argv);
            let diagnostic = serde_json::json!({
                "root": self.root,
                "canonical_root": canonical_root,
                "fixture": ready,
                "observed_process": process,
                "observer_user": table.get(std::process::id()).and_then(|process| process.user.as_ref()),
                "is_mine": table.is_mine(pid),
                "cwd_under_root": process.as_ref().and_then(|process| process.cwd.as_ref()).map(|cwd| holdmap_core::stack::path_is_under(cwd, &canonical_root)),
                "canonical_process_cwd": process.as_ref().and_then(|process| process.cwd.as_ref()).and_then(|cwd| std::fs::canonicalize(cwd).ok()),
            });
            let listed = self
                .command()
                .args(["list", "--all", "--json"])
                .args([format!("pid:{pid}"), port.to_string()])
                .output()
                .unwrap();
            panic!(
                "init did not capture owned port {port}; diagnostics={diagnostic:#}; init stdout={}; init stderr={}; owned list status={}; owned list stdout={}; owned list stderr={}; generated={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
                listed.status,
                String::from_utf8_lossy(&listed.stdout),
                String::from_utf8_lossy(&listed.stderr),
                std::fs::read_to_string(&stack.file).unwrap(),
            );
        }
        assert!(holdmap_core::stack::path_is_under(
            &stack.services[0].cwd,
            &self.cwd
        ));
        assert!(holdmap_core::stack::path_is_under(
            &self.cwd,
            &stack.services[0].cwd
        ));
        stack
    }

    fn wait_ready(&self) -> Ready {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(bytes) = std::fs::read(&self.ready) {
                if let Ok(ready) = serde_json::from_slice(&bytes) {
                    return ready;
                }
            }
            assert!(
                Instant::now() < deadline,
                "owned fixture did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wrapper(&self, line: &str) -> Command {
        let mut command = holdmap_core::util::shell_command(line);
        self.prepare(&mut command);
        let mut command = Command::from_std(command);
        command.timeout(Duration::from_secs(15));
        command
    }

    fn assert_no_injection(&self) {
        assert!(!self.cwd.join("injected.marker").exists());
        assert!(!self.root.join("injected.marker").exists());
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // Detached up fixtures observe this private control file, so cleanup never signals
        // an arbitrary PID. They also have an independent finite watchdog.
        let _ = std::fs::write(&self.quit, "quit");
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let alive = std::fs::read(&self.ready)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Ready>(&bytes).ok())
                .is_some_and(|ready| {
                    TcpStream::connect_timeout(
                        &([127, 0, 0, 1], ready.port).into(),
                        Duration::from_millis(50),
                    )
                    .is_ok()
                });
            if !alive {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn assert_free(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(50))
            .is_err()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("owned fixture port {port} remained open");
}

fn decoded_script(line: &str) -> String {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(line.split_whitespace().last().unwrap())
        .unwrap();
    let (units, remainder) = bytes.as_chunks::<2>();
    assert!(remainder.is_empty());
    let utf16: Vec<u16> = units
        .iter()
        .map(|part| u16::from_le_bytes([part[0], part[1]]))
        .collect();
    String::from_utf16(&utf16).unwrap()
}

#[test]
fn generated_windows_stack_round_trips_argv_cwd_and_environment_then_stops_only_its_listener() {
    let mut project = Project::new();
    let argv: Vec<String> = [
        "",
        "arg with spaces",
        "雪 café",
        "embedded \"quote\"",
        r#"slashes\\\"quote"#,
        r"trailing\",
        r"space trailing\",
        "%HOLDMAP_INIT_EXPAND%",
        "!",
        "$()",
        "it's",
        "a&b|c<d>e^(f)",
        "safe & echo injected>injected.marker",
        "--api-key",
        "init-fixture-secret",
    ]
    .iter()
    .map(|word| (*word).into())
    .collect();
    let original = project.start(&argv);
    let stack = project.init(original.port);
    let command = stack.services[0]
        .command
        .as_ref()
        .expect("representable argv gets a command");
    assert!(
        command.starts_with("powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand ")
    );
    let config = std::fs::read_to_string(&stack.file).unwrap();
    assert!(!config.contains("init-fixture-secret"));
    assert!(!decoded_script(command).contains("init-fixture-secret"));
    assert!(config.contains("captured argv"));
    project.stop_original();
    assert_free(original.port);

    let output = project.command().arg("up").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let restarted = project.wait_ready();
    assert_ne!(restarted.pid, original.pid);
    assert_eq!(restarted.port, original.port);
    assert_eq!(restarted.argv, holdmap_core::redact::args(&original.argv));
    assert!(holdmap_core::stack::path_is_under(
        &restarted.cwd,
        &original.cwd
    ));
    assert!(holdmap_core::stack::path_is_under(
        &original.cwd,
        &restarted.cwd
    ));
    assert_eq!(restarted.inherited, original.inherited);
    project.assert_no_injection();
    let output = project
        .command()
        .args(["status", "--no-http", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["services"][0]["state"], "running");
    assert_eq!(status["services"][0]["pid"], restarted.pid);
    let output = project
        .command()
        .args(["down", "--yes", "--timeout", "100ms"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_free(original.port);
    project.assert_no_injection();
}

#[test]
fn generated_windows_wrapper_inherits_stdio_propagates_exit_and_reports_start_failure() {
    let mut project = Project::new();
    let original = project.start(&["literal".into()]);
    let stack = project.init(original.port);
    let command = stack.services[0].command.as_ref().unwrap();
    project.stop_original();
    let output: Output = project
        .wrapper(command)
        .env("HOLDMAP_INIT_EXIT_CODE", "17")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(17), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("inherited stdout"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("inherited stderr"));
    std::fs::remove_file(&project.exe).unwrap();
    let output = project.wrapper(command).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!output.stderr.is_empty());
    assert_free(original.port);
    project.assert_no_injection();
}

#[test]
fn oversized_captured_windows_command_requires_manual_review_and_cannot_execute() {
    let mut project = Project::new();
    let original = project.start(&["long".repeat(2_000)]);
    let stack = project.init(original.port);
    assert!(stack.services[0].command.is_none());
    let config = std::fs::read_to_string(&stack.file).unwrap();
    assert!(config.contains("needs manual review"));
    assert!(config.contains("Windows shell limit"));
    project.stop_original();
    let output = project.command().arg("up").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("no `command`"));
    assert_free(original.port);
    project.assert_no_injection();
}

#[test]
#[ignore = "internal finite Windows init/up listener fixture"]
fn vite_init_listener_fixture() {
    if let Ok(code) = std::env::var("HOLDMAP_INIT_EXIT_CODE") {
        println!("inherited stdout");
        eprintln!("inherited stderr");
        std::process::exit(code.parse::<i32>().unwrap());
    }
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let ready = Ready {
        pid: std::process::id(),
        port: listener.local_addr().unwrap().port(),
        argv: std::env::args().collect(),
        cwd: std::env::current_dir().unwrap(),
        inherited: std::env::var("HOLDMAP_INIT_ENV").unwrap(),
    };
    std::fs::write(
        std::env::var_os("HOLDMAP_INIT_READY").unwrap(),
        serde_json::to_vec(&ready).unwrap(),
    )
    .unwrap();
    let quit = PathBuf::from(std::env::var_os("HOLDMAP_INIT_QUIT").unwrap());
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !Path::new(&quit).exists() {
        for _ in 0..16 {
            if listener.accept().is_err() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
