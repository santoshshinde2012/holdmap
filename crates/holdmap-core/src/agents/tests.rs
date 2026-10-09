//! Fixture tests: Cursor with a dev server in its terminal, Claude Code started from that
//! terminal, a sandboxed Codex session and an unrelated server.

use super::*;
use crate::model::*;
use crate::process::tests::{proc, table};
use crate::scan::build_entries;
use std::fs;

fn listen(addr: &str, port: u16, pid: u32) -> RawSocket {
    RawSocket {
        protocol: Protocol::Tcp,
        family: Family::V4,
        local_addr: addr.parse().unwrap(),
        local_port: port,
        remote_addr: None,
        remote_port: None,
        state: SocketState::Listen,
        uid: Some(1000),
        inode: None,
        pids: vec![pid],
    }
}

fn est(pid: u32, local: &str, lport: u16, remote: &str, rport: u16) -> RawSocket {
    RawSocket {
        protocol: Protocol::Tcp,
        family: Family::V4,
        local_addr: local.parse().unwrap(),
        local_port: lport,
        remote_addr: Some(remote.parse().unwrap()),
        remote_port: Some(rport),
        state: SocketState::Established,
        uid: Some(1000),
        inode: None,
        pids: vec![pid],
    }
}

fn at(mut p: ProcessInfo, cwd: &Path) -> ProcessInfo {
    p.cwd = Some(cwd.to_path_buf());
    p
}

fn scan_of(procs: Vec<ProcessInfo>, raw: Vec<RawSocket>, platform: &str) -> Scan {
    let t = table(procs, 99);
    let (entries, hidden) = build_entries(&raw, &t, &[], false);
    Scan {
        snapshot: Snapshot {
            entries,
            hidden_sockets: hidden,
            platform: platform.into(),
            taken_at_ms: 7,
            scan_ms: 0,
            docker_available: false,
            warnings: vec![],
        },
        table: t,
        published: vec![],
        raw,
    }
}

fn project(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("package.json"),
        format!(
            r#"{{"name":"{name}","scripts":{{"dev":"vite"}},"devDependencies":{{"vite":"6"}}}}"#
        ),
    )
    .unwrap();
    root
}

/// Cursor (100) with a helper (101), a terminal shell (110) running `vite` (112, :5173) in
/// shop-web; Claude Code (200) started from that shell in shop-api, with a server (210) on
/// 0.0.0.0:4000 that uses postgres (300), and two HTTPS connections; a re-parented Cursor
/// helper (150); Codex (400) in docs with `--sandbox workspace-write`; an unrelated server (500).
fn machine(dir: &Path) -> Scan {
    let web = project(dir, "shop-web");
    let api = project(dir, "shop-api");
    let docs = dir.join("docs");
    fs::create_dir_all(&docs).unwrap();
    let mut cursor = proc(
        100,
        1,
        "Cursor",
        &["/Applications/Cursor.app/Contents/MacOS/Cursor"],
    );
    cursor.exe = Some("/Applications/Cursor.app/Contents/MacOS/Cursor".into());
    cursor.cwd = Some("/".into());
    let procs = vec![
        proc(1, 0, "launchd", &[]),
        cursor,
        proc(101, 100, "Cursor Helper (Plugin)", &["/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Plugin).app/Contents/MacOS/Cursor Helper (Plugin)"]),
        proc(150, 1, "Cursor Helper", &["/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Cursor Helper"]),
        at(proc(110, 101, "zsh", &["-zsh"]), &web),
        at(proc(111, 110, "npm", &["npm", "run", "dev"]), &web),
        at(proc(112, 111, "node", &["node", "node_modules/.bin/vite"]), &web),
        at(
            proc(200, 110, "claude", &["claude", "--permission-mode", "acceptEdits", "--api-key=sk-live-123"]),
            &api,
        ),
        at(proc(210, 200, "node", &["node", "server.js"]), &api),
        proc(300, 1, "postgres", &["postgres", "-D", "/var/lib/pg"]),
        at(proc(400, 1, "codex", &["codex", "--sandbox", "workspace-write"]), &docs),
        proc(500, 1, "node", &["node", "other.js"]),
        proc(98, 1, "bash", &["bash"]),
        proc(99, 98, "holdmap", &["holdmap", "agents"]),
    ];
    let raw = vec![
        listen("127.0.0.1", 5173, 112),
        listen("0.0.0.0", 4000, 210),
        listen("127.0.0.1", 5432, 300),
        listen("127.0.0.1", 7000, 500),
        // Claude → Anthropic, twice.
        est(200, "10.0.0.5", 50001, "160.79.104.10", 443),
        est(200, "10.0.0.5", 50002, "160.79.104.10", 443),
        // shop-api → postgres (both halves).
        est(210, "127.0.0.1", 50100, "127.0.0.1", 5432),
        est(300, "127.0.0.1", 5432, "127.0.0.1", 50100),
        // Someone else → shop-api: the accepted side isn't one of Claude's links.
        est(500, "127.0.0.1", 50200, "127.0.0.1", 4000),
        est(210, "127.0.0.1", 4000, "127.0.0.1", 50200),
        // Cursor's helper → its own dev server: internal, not a link.
        est(101, "127.0.0.1", 50300, "127.0.0.1", 5173),
    ];
    scan_of(procs, raw, "test")
}

fn find<'a>(r: &'a AgentsReport, product: &str) -> &'a Agent {
    r.agents.iter().find(|a| a.product == product).unwrap()
}

#[test]
fn finds_each_agent_once_with_its_own_processes() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let r = AgentsBuilder::new(&scan).with_home(None).build();
    let names: Vec<&str> = r.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Claude Code", "Codex CLI", "Cursor"]);

    let cursor = find(&r, "cursor");
    assert_eq!(cursor.id, "agent:100");
    assert_eq!(cursor.kind, AgentKind::Ide);
    let pids: Vec<u32> = cursor.processes.iter().map(|p| p.pid).collect();
    assert_eq!(
        pids,
        [100, 101, 150, 110, 111, 112],
        "agent, helpers, then children"
    );
    assert_eq!(cursor.processes[1].role, ProcessRole::Helper);
    assert_eq!(cursor.processes[3].role, ProcessRole::Child);

    let claude = find(&r, "claude-code");
    assert_eq!(
        claude.parent.as_deref(),
        Some("agent:100"),
        "started from Cursor's terminal"
    );
    let pids: Vec<u32> = claude.processes.iter().map(|p| p.pid).collect();
    assert_eq!(pids, [200, 210]);
    assert!(
        !claude.command.contains("sk-live-123"),
        "secrets are hidden: {}",
        claude.command
    );
    assert!(r
        .agents
        .iter()
        .all(|a| a.processes.iter().all(|p| p.pid != 500)));
}

#[test]
fn folders_are_working_directories_with_their_projects() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    // Home = fixture root so project detection never walks into a polluted /tmp (e.g. a stray
    // package.json from another tool) and mis-labels the project-less `docs` folder.
    let r = AgentsBuilder::new(&scan)
        .with_home(Some(d.path().to_path_buf()))
        .build();
    let cursor = find(&r, "cursor");
    assert_eq!(cursor.folders.len(), 1, "/ and app bundles are skipped");
    let f = &cursor.folders[0];
    assert_eq!(
        (f.label.as_str(), f.source, f.evidence),
        ("shop-web", FolderSource::Child, Evidence::Observed)
    );
    assert_eq!(f.pids, [110, 111, 112]);
    assert_eq!(
        f.project.as_ref().map(|p| p.name.as_str()),
        Some("shop-web")
    );

    let claude = find(&r, "claude-code");
    assert_eq!(claude.folders[0].source, FolderSource::Agent);
    assert_eq!(claude.folders[0].label, "shop-api");
    let codex = find(&r, "codex");
    assert_eq!(
        codex.folders[0].label, "docs",
        "no project: the folder's own name"
    );
    assert!(codex.folders[0].project.is_none());
}

#[test]
fn ports_and_links_are_attributed_to_the_right_agent() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let r = AgentsBuilder::new(&scan).with_home(None).build();
    let cursor = find(&r, "cursor");
    assert_eq!(cursor.ports.len(), 1);
    assert_eq!(
        (cursor.ports[0].port, cursor.ports[0].role),
        (5173, PortRole::DevServer)
    );
    assert!(
        cursor.links.is_empty(),
        "helper → own dev server is internal"
    );

    let claude = find(&r, "claude-code");
    assert_eq!(
        claude.ports.iter().map(|p| p.port).collect::<Vec<_>>(),
        [4000]
    );
    let ids: Vec<&str> = claude.links.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    let pg = &claude.links[0];
    assert_eq!(
        (pg.kind, pg.port, pg.connections),
        (LinkKind::Local, 5432, 1)
    );
    assert_eq!(pg.process.as_deref(), Some("postgres"));
    assert_eq!(pg.service.as_deref(), Some("PostgreSQL"));
    let api = &claude.links[1];
    assert_eq!(
        (api.kind, api.address.as_str(), api.connections),
        (LinkKind::Remote, "160.79.104.10:443", 2)
    );
    assert_eq!(api.service.as_deref(), Some("HTTPS"));
}

#[test]
fn access_facts_are_conservative() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let r = AgentsBuilder::new(&scan).with_home(None).build();
    let claude = find(&r, "claude-code");
    let net = claude.access.fact(AccessTopic::Network).unwrap();
    assert_eq!(net.level, AccessLevel::Elevated, "{}", net.summary);
    assert!(net.summary.contains(":4000") && net.summary.contains("1 remote host"));
    let ap = claude.access.fact(AccessTopic::Approvals).unwrap();
    assert_eq!(
        (ap.level, ap.evidence),
        (AccessLevel::Standard, Evidence::Observed)
    );
    let sb = claude.access.fact(AccessTopic::Sandbox).unwrap();
    assert_eq!(sb.evidence, Evidence::Unknown);
    assert!(claude.access.mine);
    assert!(!claude.access.root);

    let codex = find(&r, "codex");
    let sb = codex.access.fact(AccessTopic::Sandbox).unwrap();
    assert_eq!(
        (sb.level, sb.evidence),
        (AccessLevel::Restricted, Evidence::Observed)
    );
    assert_eq!(
        codex.access.fact(AccessTopic::Approvals).unwrap().evidence,
        Evidence::Unknown
    );
    assert_eq!(codex.access.facts.len(), 5, "one fact per topic");
    assert!(r.limits.iter().any(|l| l.contains("never read")));
}

#[test]
fn privacy_protected_folders_are_not_inspected() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path().join("home");
    let inside = project(&home.join("Documents"), "notes-app");
    let outside = project(&home.join("code"), "shop-web");
    let procs = vec![
        proc(1, 0, "launchd", &[]),
        at(proc(200, 1, "claude", &["claude"]), &inside),
        at(proc(201, 200, "node", &["node", "x.js"]), &outside),
        proc(99, 1, "holdmap", &[]),
    ];
    let scan = scan_of(procs, vec![], "macos");
    let r = AgentsBuilder::new(&scan)
        .with_home(Some(home.clone()))
        .with_project_budget(std::time::Duration::from_secs(10))
        .build();
    let a = &r.agents[0];
    let docs = a.folders.iter().find(|f| f.path == inside).unwrap();
    assert_eq!(docs.privacy_area.as_deref(), Some("Documents"));
    assert!(docs.project.is_none(), "no reads inside a protected folder");
    assert_eq!(docs.label, "notes-app");
    assert!(docs.note.as_deref().unwrap().contains("Not inspected"));
    let code = a.folders.iter().find(|f| f.path == outside).unwrap();
    assert_eq!(
        code.project.as_ref().map(|p| p.name.as_str()),
        Some("shop-web")
    );
    let pv = a.access.fact(AccessTopic::Privacy).unwrap();
    assert_eq!(
        (pv.level, pv.evidence),
        (AccessLevel::Standard, Evidence::Inferred)
    );
    assert!(pv.summary.contains("Documents"));
    assert!(r.limits.iter().any(|l| l.contains("TCC")));
}

#[derive(Debug)]
struct Fixed(Vec<recent::RecentFolder>);
impl RecentProjects for Fixed {
    fn recent(&self, product: &str, _: &HashSet<PathBuf>) -> Vec<recent::RecentFolder> {
        if product == "claude-code" {
            self.0.clone()
        } else {
            vec![]
        }
    }
}

#[test]
fn recent_projects_follow_the_working_directories() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let api = d.path().join("shop-api");
    let src = Fixed(vec![
        recent::RecentFolder {
            path: api.clone(),
            evidence: Evidence::Observed,
            note: "Claude Code project".into(),
        },
        recent::RecentFolder {
            path: "/srv/billing".into(),
            evidence: Evidence::Inferred,
            note: "decoded".into(),
        },
    ]);
    let r = AgentsBuilder::new(&scan)
        .with_home(None)
        .with_recent(&src)
        .build();
    let claude = find(&r, "claude-code");
    let recent: Vec<&AgentFolder> = claude
        .folders
        .iter()
        .filter(|f| f.source == FolderSource::Recent)
        .collect();
    assert_eq!(
        recent.len(),
        1,
        "a recent project that's already a working directory isn't repeated"
    );
    assert_eq!(
        (recent[0].label.as_str(), recent[0].evidence),
        ("billing", Evidence::Inferred)
    );
    assert!(recent[0].project.is_none(), "recent folders aren't read");
    assert!(find(&r, "cursor")
        .folders
        .iter()
        .all(|f| f.source != FolderSource::Recent));
}

#[test]
fn serialises_with_snake_case_tags() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let r = AgentsBuilder::new(&scan).with_home(None).build();
    let v = serde_json::to_value(&r).unwrap();
    let claude = &v["agents"][0];
    assert_eq!(claude["kind"], "cli");
    assert_eq!(claude["access"]["facts"][0]["topic"], "user");
    assert_eq!(claude["links"][1]["kind"], "remote");
    assert_eq!(claude["folders"][0]["source"], "agent");
    assert!(!v.to_string().contains("sk-live-123"));
}

#[test]
fn a_machine_without_agents_has_an_empty_report() {
    let scan = scan_of(
        vec![proc(1, 0, "init", &[]), proc(99, 1, "holdmap", &[])],
        vec![],
        "linux",
    );
    let r = AgentsBuilder::new(&scan).build();
    assert!(r.agents.is_empty());
    assert_eq!(r.platform, "linux");
}
