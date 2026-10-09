//! End-to-end tests against the real OS: spawn real listeners (TCP, UDP, process trees, a
//! SIGTERM-ignoring server), then assert that holdmap lists, explains and stops them correctly.
//!
//! Helper processes are this same test binary re-executed with `HOLDMAP_HELPER` set (see
//! `helper_process`), so no external tools are needed.
#![cfg(target_os = "linux")]

use holdmap_core::*;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, UdpSocket};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

/// Entry point for helper processes. A no-op during normal test runs.
#[test]
fn helper_process() {
    let Ok(mode) = std::env::var("HOLDMAP_HELPER") else {
        return;
    };
    // Never outlive the test that spawned us, even if it panics or is interrupted.
    // SAFETY: prctl(PR_SET_PDEATHSIG) only affects this process.
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
    let (kind, arg) = mode.split_once(':').unwrap_or((mode.as_str(), ""));
    match kind {
        "tcp" | "ignore-term" => {
            if arg == "accept" {
                let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
                println!("\nPORT {}", l.local_addr().unwrap().port());
                let mut held = Vec::new();
                for s in l.incoming().flatten() {
                    held.push(s);
                }
                park(held);
            }
            if kind == "ignore-term" {
                // SAFETY: setting a signal disposition to SIG_IGN is always sound.
                unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
            }
            let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            println!("\nPORT {}", l.local_addr().unwrap().port());
            park(l);
        }
        "proxy" | "client" => {
            // Hold a live connection to the upstream port (like api → db), and for "proxy" also
            // listen (like a dev-server proxy).
            let upstream: u16 = arg.parse().unwrap();
            let conn = std::net::TcpStream::connect(("127.0.0.1", upstream)).unwrap();
            if kind == "proxy" {
                let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
                // Accept downstream clients in the background and keep them open.
                let accept = l.try_clone().unwrap();
                std::thread::spawn(move || {
                    let mut held = Vec::new();
                    for s in accept.incoming().flatten() {
                        held.push(s);
                    }
                });
                println!("\nPORT {}", l.local_addr().unwrap().port());
                park((l, conn));
            }
            println!("\nPORT {}", conn.local_addr().unwrap().port());
            park(conn);
        }
        "udp" => {
            let s = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
            println!("\nPORT {}", s.local_addr().unwrap().port());
            park(s);
        }
        "tree" => {
            // Parent process that doesn't hold the socket; its child does (like npm → node).
            let child = Helper::spawn("tcp");
            println!("\nCHILD {}", child.child.id());
            println!("PORT {}", child.port);
            let _ = arg;
            park(child);
        }
        other => panic!("unknown helper mode {other}"),
    }
}

fn park<T>(_keep: T) -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn spawn_raw_in(mode: &str, cwd: Option<&std::path::Path>) -> Child {
    // The fixture servers are children of this test process, which holdmap would otherwise
    // protect as "started by holdmap itself".
    holdmap_core::safety::set_protect_own_tree(false);
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.args([
        "--exact",
        "helper_process",
        "--nocapture",
        "--test-threads=1",
    ])
    .env("HOLDMAP_HELPER", mode)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn helper")
}

struct Helper {
    child: Child,
    port: u16,
    child_pid: u32,
    _out: BufReader<ChildStdout>,
}

impl Helper {
    fn spawn(mode: &str) -> Helper {
        Self::spawn_in(mode, None)
    }

    fn spawn_in(mode: &str, cwd: Option<&std::path::Path>) -> Helper {
        let mut child = spawn_raw_in(mode, cwd);
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let mut child_pid = 0;
        let port = loop {
            let mut line = String::new();
            assert!(out.read_line(&mut line).unwrap() > 0, "helper exited early");
            if let Some(i) = line.find("CHILD ") {
                child_pid = line[i + 6..].trim().parse().unwrap();
            }
            if let Some(i) = line.find("PORT ") {
                break line[i + 5..].trim().parse().unwrap();
            }
        };
        Helper {
            child,
            port,
            child_pid,
            _out: out,
        }
    }
    fn pid(&self) -> u32 {
        self.child.id()
    }
    fn exited(&mut self) -> bool {
        for _ in 0..50 {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn engine() -> Engine {
    Engine::new(&ScanOptions {
        all_states: false,
        docker: false,
    })
    .unwrap()
}

#[test]
fn lists_and_explains_a_real_tcp_listener() {
    let h = Helper::spawn("tcp");
    let e = engine();
    let entry = e
        .snapshot()
        .entries
        .iter()
        .find(|x| x.port == h.port && x.protocol == Protocol::Tcp)
        .expect("listener visible");
    assert_eq!(entry.pid, Some(h.pid()));
    assert_eq!(entry.state, SocketState::Listen);
    assert_eq!(entry.addresses, vec!["127.0.0.1"]);
    assert_eq!(entry.exposure, Exposure::Loopback);
    assert!(entry.is_mine);
    assert!(!entry.protected);
    let p = entry.process.as_ref().unwrap();
    assert!(p.start_token > 0);
    assert!(p.cmdline.iter().any(|a| a == "helper_process"));

    let ex = e.explain(h.port, &StopOptions::default());
    assert_eq!(ex.status, PortStatus::Busy);
    assert!(
        ex.headline.contains(&format!("PID {}", h.pid())),
        "{}",
        ex.headline
    );
    assert!(ex.plan.as_ref().unwrap().blocked.is_none());
}

#[test]
fn query_filter_finds_port() {
    let h = Helper::spawn("tcp");
    let e = engine();
    let f = Filter {
        query: format!(":{}", h.port),
        ..Default::default()
    };
    assert_eq!(f.apply(&e.snapshot().entries).len(), 1);
}

#[test]
fn sees_udp_sockets() {
    let h = Helper::spawn("udp");
    let e = engine();
    let entry = e
        .snapshot()
        .entries
        .iter()
        .find(|x| x.port == h.port && x.protocol == Protocol::Udp)
        .unwrap();
    assert_eq!(entry.state, SocketState::Bound);
    assert_eq!(entry.pid, Some(h.pid()));
}

#[test]
fn graceful_stop_frees_the_port() {
    let mut h = Helper::spawn("tcp");
    assert!(port_busy(h.port, Protocol::Tcp));
    let plan = engine().plan(&Target::Port(h.port), &StopOptions::default());
    assert!(!plan.is_blocked(), "{plan:?}");
    // Planning (dry-run) never touches the process.
    assert!(h.child.try_wait().unwrap().is_none());
    let report = execute(&plan, &mut |_| {});
    assert!(report.success, "{report:?}");
    assert!(report.freed);
    assert!(!report.escalated, "SIGTERM should have been enough");
    assert!(report.signalled.contains(&h.pid()));
    assert!(h.exited());
    assert!(!port_busy(h.port, Protocol::Tcp));
}

#[test]
fn escalates_to_sigkill_when_sigterm_is_ignored() {
    let mut h = Helper::spawn("ignore-term");
    let opts = StopOptions {
        timeout_ms: 400,
        ..Default::default()
    };
    let plan = engine().plan(&Target::Port(h.port), &opts);
    let report = execute(&plan, &mut |_| {});
    assert!(report.success, "{report:?}");
    assert!(report.escalated);
    assert!(h.exited());
}

#[test]
fn force_kills_immediately() {
    let mut h = Helper::spawn("ignore-term");
    let opts = StopOptions {
        force: true,
        ..Default::default()
    };
    let plan = engine().plan(&Target::Port(h.port), &opts);
    let started = std::time::Instant::now();
    let report = execute(&plan, &mut |_| {});
    assert!(report.success, "{report:?}");
    assert!(!report.escalated);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(h.exited());
}

#[test]
fn stops_the_whole_process_tree() {
    let mut h = Helper::spawn("tree");
    let child_pid = h.child_pid;
    let e = engine();
    let ex = e.explain(h.port, &StopOptions::default());
    match &ex.owners[0] {
        Owner::ProcessTree { root_pid, pids, .. } => {
            assert_eq!(
                *root_pid,
                h.pid(),
                "climbed from socket holder to its launcher"
            );
            assert!(pids.contains(&child_pid));
        }
        o => panic!("expected a process tree, got {o:?}"),
    }
    let report = execute(ex.plan.as_ref().unwrap(), &mut |_| {});
    assert!(report.success, "{report:?}");
    assert!(report.signalled.contains(&h.pid()) && report.signalled.contains(&child_pid));
    assert!(h.exited());
    assert!(
        !sys::is_alive(child_pid, sys::start_token(child_pid).unwrap_or(0))
            || sys::start_token(child_pid).is_none()
    );
}

#[test]
fn pid_reuse_guard_refuses_stale_identity() {
    let mut h = Helper::spawn("tcp");
    let mut plan = engine().plan(
        &Target::Port(h.port),
        &StopOptions {
            timeout_ms: 200,
            verify_timeout_ms: 200,
            ..Default::default()
        },
    );
    // Simulate the PID being recycled by another process between plan and execute.
    for step in &mut plan.steps {
        if let Step::SignalProcesses { processes, .. } = step {
            for p in processes {
                p.start_token += 1;
            }
        }
    }
    let report = execute(&plan, &mut |_| {});
    assert!(!report.success);
    assert!(report.signalled.is_empty());
    assert!(report.log.iter().any(|l| l.contains("PID reuse")));
    assert!(
        h.child.try_wait().unwrap().is_none(),
        "process must still be alive"
    );
}

#[test]
fn refuses_to_stop_own_process() {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let plan = engine().plan(
        &Target::Port(port),
        &StopOptions {
            allow_protected: true,
            ..Default::default()
        },
    );
    assert_eq!(plan.blocked.unwrap().kind, BlockKind::Protected);
}

#[test]
fn stop_by_pid() {
    let mut h = Helper::spawn("tcp");
    let plan = engine().plan(&Target::Pid(h.pid()), &StopOptions::default());
    assert!(plan
        .steps
        .iter()
        .any(|s| matches!(s, Step::VerifyFree { port, .. } if *port == h.port)));
    let report = execute(&plan, &mut |_| {});
    assert!(report.success, "{report:?}");
    assert!(h.exited());
}

/// A real three-tier stack (web → api → db) in one workspace: the topology must show the live
/// connections, group the stack, and stop it dependents-first until every port is free.
#[test]
fn topology_of_real_connected_services_and_cluster_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("e2e-stack");
    for d in ["db", "api", "web"] {
        std::fs::create_dir_all(ws.join(d)).unwrap();
        std::fs::write(
            ws.join(d).join("package.json"),
            format!("{{\"name\":\"{d}\"}}"),
        )
        .unwrap();
    }
    std::fs::write(ws.join("pnpm-workspace.yaml"), "packages: ['*']\n").unwrap();
    let mut db = Helper::spawn_in("tcp:accept", Some(&ws.join("db")));
    let mut api = Helper::spawn_in(&format!("proxy:{}", db.port), Some(&ws.join("api")));
    let mut web = Helper::spawn_in(&format!("client:{}", api.port), Some(&ws.join("web")));
    // Give the kernel a moment to show both halves of each connection.
    std::thread::sleep(Duration::from_millis(150));

    let e = engine();
    let g = e.topology();
    let id = |pid: u32| format!("svc:{pid}");
    let edge = |from: u32, to: u32| {
        g.edges
            .iter()
            .find(|x| x.from == id(from) && x.to == id(to))
            .cloned()
    };
    let wa = edge(web.pid(), api.pid()).expect("web → api edge");
    assert_eq!((wa.kind, wa.port), (topology::EdgeKind::Local, api.port));
    let ad = edge(api.pid(), db.pid()).expect("api → db edge");
    assert_eq!(ad.port, db.port);
    assert!(
        edge(db.pid(), api.pid()).is_none(),
        "no reverse edge from the server side"
    );
    assert_eq!(
        g.node(&id(web.pid())).unwrap().kind,
        topology::NodeKind::Client
    );
    let cluster = g.find_cluster("e2e-stack").expect("workspace cluster");
    assert_eq!(cluster.kind, topology::ClusterKind::Workspace);
    assert_eq!(cluster.nodes.len(), 3, "{:?}", cluster.nodes);

    // Stopping only the db warns that api depends on it.
    let p = e.plan(&Target::Port(db.port), &StopOptions::default());
    assert!(
        p.warnings.iter().any(|w| w.contains("depends on")),
        "{:?}",
        p.warnings
    );

    let plan = e.plan(&Target::parse("cluster:e2e-stack"), &StopOptions::default());
    assert!(!plan.is_blocked(), "{plan:?}");
    let order: Vec<u32> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::SignalProcesses { processes, .. } => processes.first().map(|p| p.pid),
            _ => None,
        })
        .collect();
    assert_eq!(order, [web.pid(), api.pid(), db.pid()], "dependents first");
    let report = execute(&plan, &mut |_| {});
    assert!(report.success && report.freed, "{report:?}");
    assert!(web.exited() && api.exited() && db.exited());
    assert!(!port_busy(api.port, Protocol::Tcp) && !port_busy(db.port, Protocol::Tcp));
}
