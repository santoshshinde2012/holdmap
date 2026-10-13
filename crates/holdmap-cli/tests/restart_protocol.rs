//! Port restart is a TCP operation and must preserve a separate UDP owner on the same port.

use assert_cmd::Command;
use holdmap_core::history::HistoryEntry;
use holdmap_core::store::Store;
use holdmap_core::Protocol;
use serde::{Deserialize, Serialize};
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize, Serialize)]
struct Ready {
    pid: u32,
    port: u16,
}

struct Owned {
    directory: tempfile::TempDir,
    exe: PathBuf,
    children: Vec<Child>,
    restarted: Option<(u32, u64)>,
    // Keep the immutable executable alias until all owned listeners have been cleaned up.
    #[cfg(unix)]
    _executable_directory: tempfile::TempDir,
}

impl Owned {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "owned-listener.exe"
        } else {
            "owned-listener"
        };
        let current_exe = std::env::current_exe().unwrap();
        #[cfg(unix)]
        let executable_directory = tempfile::Builder::new()
            .prefix("hm-restart-bin-")
            .tempdir_in(current_exe.parent().unwrap())
            .unwrap();
        #[cfg(unix)]
        let exe = {
            // A concurrent fork may retain a copied ELF's writable descriptor until
            // exec, producing Linux ETXTBSY. Link the running binary on the same
            // filesystem without opening it for writing; keep the service cwd isolated.
            let exe = executable_directory.path().join(name);
            std::fs::hard_link(&current_exe, &exe).unwrap();
            exe
        };
        #[cfg(not(unix))]
        let exe = {
            let exe = directory.path().join(name);
            std::fs::copy(&current_exe, &exe).unwrap();
            exe
        };
        Self {
            directory,
            exe,
            children: vec![],
            restarted: None,
            #[cfg(unix)]
            _executable_directory: executable_directory,
        }
    }

    fn state(&self) -> PathBuf {
        self.directory.path().join("state")
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("holdmap").unwrap();
        command
            .current_dir(self.directory.path())
            .env("HOLDMAP_HOME", self.state())
            .env("HOLDMAP_COLOR", "never")
            .arg("--no-docker")
            .timeout(Duration::from_secs(25));
        command
    }

    fn start(&mut self, protocol: &str, port: u16) -> Ready {
        let port_file = self.directory.path().join(format!("{protocol}.port"));
        std::fs::write(&port_file, port.to_string()).unwrap();
        let ready = self.directory.path().join(format!("{protocol}.json"));
        let child = std::process::Command::new(&self.exe)
            .args([
                "--ignored",
                "--exact",
                "owned_listener_fixture",
                "--nocapture",
                "--",
                protocol,
            ])
            .arg(&port_file)
            .arg(&ready)
            .arg(self.directory.path().join("quit"))
            .current_dir(self.directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        self.children.push(child);
        wait_ready(&ready, |ready| ready.pid == pid)
    }

    fn marker_entry(&self, at_ms: u64, port: u16, protocol: Protocol, pid: u32) -> HistoryEntry {
        HistoryEntry {
            at_ms,
            port,
            protocol,
            label: "owned recorded marker".into(),
            command: vec![
                self.exe.to_string_lossy().into_owned(),
                "--ignored".into(),
                "--exact".into(),
                "owned_marker_fixture".into(),
                "--nocapture".into(),
                "--".into(),
                self.directory
                    .path()
                    .join("launched.marker")
                    .to_string_lossy()
                    .into_owned(),
            ],
            cwd: Some(self.directory.path().to_path_buf()),
            project: None,
            framework: None,
            pid,
            windows_restart: None,
            restartable: None,
            copyable: None,
        }
    }

    fn assert_no_marker(&self) {
        assert!(!self.directory.path().join("launched.marker").exists());
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::write(self.directory.path().join("quit"), "quit");
        if let Some((pid, token)) = self.restarted {
            let _ = holdmap_core::sys::signal(pid, token, holdmap_core::sys::Sig::Kill);
        }
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_ready(path: &Path, matches: impl Fn(&Ready) -> bool) -> Ready {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(ready) = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .filter(&matches)
        {
            return ready;
        }
        assert!(
            Instant::now() < deadline,
            "owned listener did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_udp_alive(port: u16) {
    let probe = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    probe.send_to(b"owned-probe", ("127.0.0.1", port)).unwrap();
    let mut bytes = [0u8; 32];
    let (length, _) = probe.recv_from(&mut bytes).unwrap();
    assert_eq!(&bytes[..length], b"owned-probe");
}

#[test]
fn restarting_an_owned_tcp_listener_preserves_a_separate_udp_process_on_the_same_port() {
    let mut owned = Owned::new();
    let tcp = owned.start("tcp", 0);
    let udp = owned.start("udp", tcp.port);
    assert_udp_alive(tcp.port);
    let output = owned
        .command()
        .args(["restart", &tcp.port.to_string(), "--yes", "--timeout", "5s"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let restarted = wait_ready(&owned.directory.path().join("tcp.json"), |ready| {
        ready.pid != tcp.pid
    });
    assert_eq!(restarted.port, tcp.port);
    let token = holdmap_core::sys::start_token(restarted.pid).expect("owned restarted identity");
    owned.restarted = Some((restarted.pid, token));
    assert_udp_alive(tcp.port);
    assert!(
        owned.children[1].try_wait().unwrap().is_none(),
        "UDP owner {} was stopped",
        udp.pid
    );
}

#[test]
fn udp_history_cannot_launch_a_command_for_a_free_tcp_port() {
    let mut owned = Owned::new();
    let udp = owned.start("udp", 0);
    Store::new(owned.state())
        .record(&[owned.marker_entry(10, udp.port, Protocol::Udp, udp.pid)])
        .unwrap();
    let output = owned
        .command()
        .args([
            "restart",
            &udp.port.to_string(),
            "--yes",
            "--timeout",
            "100ms",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    owned.assert_no_marker();
    assert_udp_alive(udp.port);
    assert!(owned.children[0].try_wait().unwrap().is_none());
}

#[test]
fn newest_unrestartable_tcp_history_cannot_fall_back_to_an_older_command_or_newer_udp_record() {
    let owned = Owned::new();
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let older = owned.marker_entry(10, port, Protocol::Tcp, 100);
    let mut latest_tcp = owned.marker_entry(20, port, Protocol::Tcp, 200);
    latest_tcp.command.clear();
    let latest_udp = owned.marker_entry(30, port, Protocol::Udp, 300);
    Store::new(owned.state())
        .record(&[older, latest_tcp, latest_udp])
        .unwrap();
    let output = owned
        .command()
        .args(["restart", &port.to_string(), "--yes", "--timeout", "100ms"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    owned.assert_no_marker();
}

#[test]
#[ignore = "internal finite owned TCP/UDP listener fixture"]
fn owned_listener_fixture() {
    let argv: Vec<_> = std::env::args_os().collect();
    let start = argv.iter().position(|arg| arg == "--").unwrap() + 1;
    let protocol = argv[start].to_str().unwrap();
    let port_file = Path::new(&argv[start + 1]);
    let ready_file = Path::new(&argv[start + 2]);
    let quit = Path::new(&argv[start + 3]);
    let port = std::fs::read_to_string(port_file)
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let tcp = (protocol == "tcp").then(|| TcpListener::bind(("127.0.0.1", port)).unwrap());
    let udp = (protocol == "udp").then(|| UdpSocket::bind(("127.0.0.1", port)).unwrap());
    let port = tcp
        .as_ref()
        .map(|s| s.local_addr().unwrap().port())
        .or_else(|| udp.as_ref().map(|s| s.local_addr().unwrap().port()))
        .unwrap();
    if let Some(tcp) = &tcp {
        tcp.set_nonblocking(true).unwrap();
    }
    if let Some(udp) = &udp {
        udp.set_nonblocking(true).unwrap();
    }
    std::fs::write(port_file, port.to_string()).unwrap();
    std::fs::write(
        ready_file,
        serde_json::to_vec(&Ready {
            pid: std::process::id(),
            port,
        })
        .unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !quit.exists() {
        if let Some(tcp) = &tcp {
            for _ in 0..16 {
                if tcp.accept().is_err() {
                    break;
                }
            }
        }
        if let Some(udp) = &udp {
            let mut bytes = [0u8; 32];
            for _ in 0..16 {
                let Ok((length, remote)) = udp.recv_from(&mut bytes) else {
                    break;
                };
                udp.send_to(&bytes[..length], remote).unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "internal finite marker proving unintended history execution"]
fn owned_marker_fixture() {
    let argv: Vec<_> = std::env::args_os().collect();
    let start = argv.iter().position(|arg| arg == "--").unwrap() + 1;
    std::fs::write(&argv[start], "launched").unwrap();
}
