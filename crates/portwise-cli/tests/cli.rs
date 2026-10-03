//! End-to-end tests for the `portwise` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::net::TcpListener;
use std::time::Duration;

fn pw() -> Command {
    let mut c = Command::cargo_bin("portwise").unwrap();
    c.env("PORTWISE_COLOR", "never").arg("--no-docker");
    c
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn help_mentions_examples_and_exit_codes() {
    pw().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES"))
        .stdout(predicate::str::contains("EXIT CODES"));
}

#[test]
fn list_json_is_valid_and_contains_our_listener() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let out = pw().args(["list", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let entries = v["entries"].as_array().unwrap();
    let ours = entries
        .iter()
        .find(|e| e["port"] == port)
        .expect("our listener is listed");
    assert_eq!(ours["protocol"], "tcp");
    assert_eq!(ours["pid"], std::process::id());
}

#[test]
fn list_plain_table_has_header() {
    let _l = TcpListener::bind("127.0.0.1:0").unwrap();
    pw().args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PORT"));
}

#[test]
fn explain_free_port_succeeds() {
    let port = free_port();
    pw().args(["explain", &port.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("is free"));
}

#[test]
fn explain_busy_port_names_the_owner() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let out = pw()
        .args(["explain", &port.to_string(), "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "busy");
    assert!(v["headline"]
        .as_str()
        .unwrap()
        .contains(&std::process::id().to_string()));
}

#[test]
fn stop_refuses_to_kill_itself_or_its_parent() {
    // The listener belongs to the test process, which is portwise's parent: must be blocked.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    pw().args(["stop", &port.to_string(), "--yes"])
        .assert()
        .code(3);
    // …and we're still alive with the socket open.
    assert!(l.local_addr().is_ok());
}

#[test]
fn stop_dry_run_on_free_port_reports_nothing_to_do() {
    let port = free_port();
    pw().args(["stop", &port.to_string(), "--dry-run"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("free").or(predicate::str::contains("Nothing")));
}

#[test]
fn free_port_returns_bindable_ports() {
    let out = pw()
        .args(["free-port", "--count", "3", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ports = v["ports"].as_array().unwrap();
    assert_eq!(ports.len(), 3);
    for p in ports {
        TcpListener::bind(("127.0.0.1", p.as_u64().unwrap() as u16)).expect("port is free");
    }
}

#[test]
fn free_port_near_skips_busy() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let out = pw()
        .args(["free-port", "--near", &port.to_string()])
        .output()
        .unwrap();
    let got: u16 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    assert!(got > port);
}

#[test]
fn wait_times_out_with_exit_1() {
    let port = free_port();
    pw().args(["wait", &port.to_string(), "--timeout", "300ms", "--quiet"])
        .timeout(Duration::from_secs(10))
        .assert()
        .code(1);
}

#[test]
fn wait_succeeds_when_listening() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    pw().args(["wait", &port.to_string(), "--timeout", "3s"])
        .assert()
        .success();
}

#[test]
fn wait_free_succeeds_on_free_port() {
    let port = free_port();
    pw().args(["wait", &port.to_string(), "--free", "--timeout", "1s", "-q"])
        .assert()
        .success();
}

#[test]
fn bad_target_is_a_usage_error() {
    pw().args(["explain", "not-a-port"]).assert().failure();
}

#[test]
fn completions_and_man_render() {
    pw().args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("portwise"));
    pw().arg("man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH"));
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::process::{Child, Stdio};

    fn python_listener(port: u16, ignore_term: bool) -> Option<Child> {
        let code = format!(
            "import socket,signal,time\n{}s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\ns.bind(('127.0.0.1',{port}));s.listen()\ntime.sleep(60)",
            if ignore_term { "signal.signal(signal.SIGTERM, signal.SIG_IGN)\n" } else { "" }
        );
        let child = std::process::Command::new("python3")
            .args(["-c", &code])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Some(child);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    #[test]
    fn stop_frees_a_real_listener() {
        let port = free_port();
        let Some(mut child) = python_listener(port, false) else {
            eprintln!("python3 not available; skipping");
            return;
        };
        pw().args(["stop", &port.to_string(), "--yes"])
            .timeout(Duration::from_secs(20))
            .assert()
            .success()
            .stdout(predicate::str::contains("free"));
        let _ = child.wait();
        TcpListener::bind(("127.0.0.1", port)).expect("port was freed");
    }

    #[test]
    fn stop_escalates_to_sigkill_when_sigterm_is_ignored() {
        let port = free_port();
        let Some(mut child) = python_listener(port, true) else {
            return;
        };
        pw().args([
            "stop",
            &port.to_string(),
            "--yes",
            "--timeout",
            "500ms",
            "--json",
        ])
        .timeout(Duration::from_secs(20))
        .assert()
        .success()
        .stdout(predicate::str::contains("SIGKILL"));
        let _ = child.wait();
    }

    #[test]
    fn run_frees_port_then_execs_with_port_env() {
        let port = free_port();
        let Some(mut child) = python_listener(port, false) else {
            return;
        };
        pw().args([
            "run",
            "-p",
            &port.to_string(),
            "--yes",
            "--",
            "sh",
            "-c",
            "echo got=$PORT",
        ])
        .timeout(Duration::from_secs(20))
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("got={port}")));
        let _ = child.wait();
    }
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn graph_exports_json_dot_and_mermaid() {
    let out = pw().args(["graph", "--all", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["nodes"].is_array() && v["edges"].is_array() && v["clusters"].is_array());
    pw().args(["graph", "--all", "--dot"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("digraph portwise {"));
    pw().args(["mesh", "--all", "--format", "mermaid"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("flowchart LR"));
    pw().args(["graph", "--cluster", "definitely-not-a-cluster"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no cluster named"));
}

#[test]
fn graph_shows_a_live_connection_between_two_processes() {
    if std::process::Command::new("python3")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mut client = std::process::Command::new("python3")
        .args([
            "-c",
            &format!("import socket,time; s=socket.create_connection(('127.0.0.1',{port})); print('ok', flush=True); time.sleep(30)"),
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let (_conn, _) = l.accept().unwrap();
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(client.stdout.as_mut().unwrap()),
        &mut line,
    )
    .unwrap();
    let out = pw().args(["graph", "--all", "--json"]).output().unwrap();
    let _ = client.kill();
    let _ = client.wait();
    let g: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let client_id = format!("svc:{}", client.id());
    let edge = g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["from"] == client_id.as_str() && e["port"] == port)
        .unwrap_or_else(|| panic!("no edge from {client_id} to :{port}: {}", g["edges"]));
    assert_eq!(edge["kind"], "local");
}

#[test]
fn pins_round_trip_in_an_isolated_home() {
    let h = home();
    pw().env("PORTWISE_HOME", h.path())
        .args(["pin", "3999", "--label", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pinned :3999"));
    let out = pw()
        .env("PORTWISE_HOME", h.path())
        .args(["pins", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["port"], 3999);
    assert_eq!(v[0]["label"], "demo");
    pw().env("PORTWISE_HOME", h.path())
        .args(["unpin", "3999"])
        .assert()
        .success();
    pw().env("PORTWISE_HOME", h.path())
        .args(["pins", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn history_restart_and_open() {
    let h = home();
    pw().env("PORTWISE_HOME", h.path())
        .args(["history", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
    let port = free_port();
    pw().env("PORTWISE_HOME", h.path())
        .args(["restart", &port.to_string()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no record"));
    pw().args(["open", "--print", "3000"])
        .assert()
        .success()
        .stdout("http://localhost:3000\n");
}

#[test]
fn stop_unknown_cluster_is_nothing_to_stop() {
    pw().args(["stop", "--cluster", "nope", "--dry-run"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("No cluster named"));
}

#[cfg(target_os = "linux")]
#[test]
fn ssh_agentless_local_lists_ports() {
    if std::process::Command::new("ss").arg("-V").output().is_err() {
        return;
    }
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let out = pw()
        .args(["ssh", "local", "list", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["platform"], "remote:local");
    assert!(v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["port"] == port));
}

#[test]
fn watch_runs_for_a_bounded_number_of_polls() {
    pw().args(["watch", "--json", "--polls", "2", "--interval", "50ms"])
        .assert()
        .success();
}
