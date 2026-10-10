//! Security regressions exercise the shipped CLI using isolated files and owned fixtures.

use assert_cmd::Command;
use std::net::TcpListener;
use std::time::Duration;

fn command(state: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("holdmap").unwrap();
    command
        .env("HOLDMAP_HOME", state)
        .env("HOLDMAP_COLOR", "never")
        .arg("--no-docker")
        .timeout(Duration::from_secs(15));
    command
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn run_redacts_displayed_arguments_before_exec_failure() {
    let directory = tempfile::tempdir().unwrap();
    let output = command(&directory.path().join("state"))
        .args([
            "run",
            "--port",
            &free_port().to_string(),
            "--",
            "holdmap-security-fixture-not-an-executable",
            "--api-key",
            "security-fixture-secret",
            "https://fixture:credential@localhost/path",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("--api-key"));
    assert!(text.contains("••••"));
    assert!(!text.contains("security-fixture-secret"));
    assert!(!text.contains("credential"));

    let output = command(&directory.path().join("state"))
        .args([
            "run",
            "--port",
            &free_port().to_string(),
            "--",
            "https://fixture:credential@localhost/not-an-executable",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("failed to run"));
    assert!(!text.contains("credential"));
}

#[test]
fn stack_dry_run_redacts_command_without_executing_it() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join(".holdmap.toml");
    std::fs::write(
        &config,
        format!(
            "[services.web]\nport = {}\ncommand = 'fixture-command --api-key security-fixture-secret --password \"multiword fixture secret\"'\n",
            free_port()
        ),
    )
    .unwrap();
    let output = command(&directory.path().join("state"))
        .arg("up")
        .arg("--file")
        .arg(config)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("would run"));
    assert!(text.contains("fixture-command"));
    assert!(text.contains("••••"));
    assert!(!text.contains("security-fixture-secret"));
    assert!(!text.contains("multiword fixture secret"));
    assert!(!text.contains("fixture secret"));
    assert!(!directory.path().join("state/logs").exists());
}

// Run this test executable as a controlled listener, without depending on Python or a shell.
// It is ignored during the normal suite and selected explicitly by the parent regression.
#[test]
#[ignore = "internal subprocess fixture"]
fn terminal_listener_fixture() {
    let port_file = std::env::var_os("HOLDMAP_SECURITY_FIXTURE_PORT_FILE")
        .expect("the parent supplied a port file");
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    std::fs::write(port_file, listener.local_addr().unwrap().port().to_string()).unwrap();
    std::io::Read::read_to_end(&mut std::io::stdin().lock(), &mut Vec::new()).unwrap();
}

#[cfg(unix)]
#[test]
fn inspect_escapes_untrusted_working_directory_terminal_controls() {
    use std::process::{Child, Stdio};
    use std::time::Instant;

    struct OwnedListener(Child);
    impl Drop for OwnedListener {
        fn drop(&mut self) {
            drop(self.0.stdin.take());
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let project = directory
        .path()
        .join("project-\x1b]52;c;cGF5bG9hZA==\x07\nforged-row");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{"name":"terminal-security-fixture","version":"1.0.0"}"#,
    )
    .unwrap();
    let port_file = directory.path().join("listener.port");
    let _listener = OwnedListener(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "terminal_listener_fixture",
                "--nocapture",
            ])
            .env("HOLDMAP_SECURITY_FIXTURE_PORT_FILE", &port_file)
            .current_dir(&project)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let port: u16 = loop {
        if let Ok(port) = std::fs::read_to_string(&port_file)
            .and_then(|text| text.parse::<u16>().map_err(std::io::Error::other))
        {
            break port;
        }
        assert!(Instant::now() < deadline, "fixture listener did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    for color in ["never", "always"] {
        let output = command(&directory.path().join("state"))
            .args(["inspect", &port.to_string(), "--color", color])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("Working dir"));
        assert!(text.contains("project-"));
        assert!(text.contains("forged-row"));
        assert!(!text.contains("\x1b]52;"));
        assert!(!text.contains('\x07'));
        assert!(!text.contains("\nforged-row"));
        if color == "always" {
            assert!(text.contains("\x1b[1m"), "application styling is retained");
        } else {
            assert!(!text.contains('\x1b'));
        }
    }
}

#[cfg(unix)]
#[test]
fn generated_project_and_man_files_do_not_overwrite_link_targets() {
    use std::os::unix::fs::symlink;

    for hard_link in [false, true] {
        for (name, args) in [
            (".holdmap.toml", vec!["init", "--force"]),
            ("holdmap.1", vec!["man", "--out-dir", "."]),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let project = directory.path().join("project");
            std::fs::create_dir(&project).unwrap();
            let unrelated = directory.path().join("unrelated-file");
            let original = "unrelated fixture must remain untouched";
            std::fs::write(&unrelated, original).unwrap();
            if hard_link {
                std::fs::hard_link(&unrelated, project.join(name)).unwrap();
            } else {
                symlink(&unrelated, project.join(name)).unwrap();
            }
            let output = command(&directory.path().join("state"))
                .current_dir(&project)
                .args(args)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), original);
        }
    }
}

#[cfg(unix)]
#[test]
fn ssh_forwarding_normalizes_destination_and_quotes_remote_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let bin = directory.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let ssh = bin.join("ssh");
    // Simulate OpenSSH's concatenated remote command in an isolated local shell. Only
    // fixture executables are selected; the test never opens an SSH connection.
    std::fs::write(
        &ssh,
        r#"#!/bin/sh
port=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift 2 ;;
    -p) port=$2; shift 2 ;;
    -t) shift ;;
    --) shift; break ;;
    *) exit 80 ;;
  esac
done
[ "$port" = 2222 ] && [ "$1" = 'user@fixture.invalid' ] || exit 81
shift
exec /bin/sh -c "$*"
"#,
    )
    .unwrap();
    let remote_holdmap = bin.join("holdmap");
    std::fs::write(
        &remote_holdmap,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOLDMAP_SECURITY_REMOTE_ARGS\"\n",
    )
    .unwrap();
    for path in [&ssh, &remote_holdmap] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let marker = directory.path().join("must-not-exist");
    let argument = format!("literal'; touch {} ; printf '", marker.display());
    let captured = directory.path().join("remote.args");
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = command(&directory.path().join("state"))
        .env("PATH", path)
        .env("HOLDMAP_SECURITY_REMOTE_ARGS", &captured)
        .args(["ssh", " user@fixture.invalid:2222 ", "list", &argument])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(captured).unwrap(),
        format!("list\n{argument}\n")
    );
    assert!(
        !marker.exists(),
        "argument text must not become remote shell code"
    );
}
