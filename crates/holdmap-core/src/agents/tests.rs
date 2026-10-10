//! Fixture tests: Cursor with a dev server in its terminal, Claude Code started from that
//! terminal, a sandboxed Codex session and an unrelated server.

use super::*;
use crate::model::*;
use crate::process::tests::{proc, table};
use crate::scan::{build_entries, Scan};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Project lookups in these fixtures never compete for the production global cache.
fn agents_builder(scan: &Scan) -> AgentsBuilder<'_> {
    use crate::project::{detect_project, ProjectCache};
    use std::sync::Arc;
    use std::time::Duration;
    AgentsBuilder::new(scan).with_project_cache(ProjectCache::new(
        Arc::new(detect_project),
        Duration::from_secs(60),
    ))
}

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
    let r = agents_builder(&scan).with_home(None).build();
    let names: Vec<&str> = r.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Claude Code", "Codex CLI", "Cursor"]);

    let cursor = find(&r, "cursor");
    assert_eq!(cursor.id, "agent:100");
    assert_eq!(cursor.kind, AgentKind::Ide);
    let pids: Vec<u32> = cursor.processes.iter().map(|p| p.pid).collect();
    assert_eq!(
        pids,
        [100, 110, 111, 112, 101, 150],
        "agent, useful children, then helpers"
    );
    assert_eq!(cursor.processes[4].role, ProcessRole::Helper);
    assert_eq!(cursor.processes[1].role, ProcessRole::Child);

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
fn editor_helpers_merge_only_with_a_known_matching_account() {
    let process = |pid, name, user: Option<&str>| {
        let mut p = proc(pid, 1, name, &[name]);
        p.uid = None; // Windows has no numeric UID.
        p.user = user.map(str::to_owned);
        p
    };
    let scan = scan_of(
        vec![
            process(100, "Cursor", Some("work\\alice")),
            process(101, "Cursor Helper", Some("work\\alice")),
            process(200, "Cursor", Some("work\\bob")),
            process(201, "Cursor Helper", Some("work\\bob")),
            process(300, "Cursor", None),
            process(400, "Cursor", None),
        ],
        vec![],
        "windows",
    );
    let report = agents_builder(&scan)
        .with_home(None)
        .with_project_detection(false)
        .build();
    let owned: Vec<_> = report
        .agents
        .iter()
        .map(|a| (a.pid, a.process_ids.clone()))
        .collect();
    assert_eq!(
        owned,
        [
            (100, vec![100, 101]),
            (200, vec![200, 201]),
            (300, vec![300]),
            (400, vec![400])
        ],
        "other accounts and unknown identities retain independent roots"
    );
}

#[test]
fn developer_tool_helpers_fold_and_keep_upstream_port_and_folder_controls() {
    let workspace = PathBuf::from("/fixture/tool-workspace");
    let scan = scan_of(
        vec![
            proc(100, 1, "Docker Desktop", &["Docker Desktop"]),
            proc(101, 1, "com.docker.backend", &["com.docker.backend"]),
            at(proc(200, 101, "node", &["node", "server.js"]), &workspace),
            proc(99, 1, "holdmap", &["holdmap"]),
        ],
        vec![
            listen("127.0.0.1", 5000, 100),
            listen("127.0.0.1", 4000, 200),
        ],
        "fixture",
    );
    let report = agents_builder(&scan).with_project_detection(false).build();
    assert_eq!(report.agents.len(), 1, "same-account runtime helpers fold");
    let tool = &report.agents[0];
    assert_eq!(tool.kind, AgentKind::Tool);
    assert_eq!(tool.product, "docker-desktop");
    assert_eq!(tool.process_ids, [100, 101, 200]);
    assert_eq!(
        tool.stoppable_ports().map(|p| p.port).collect::<Vec<_>>(),
        [4000]
    );
    assert_eq!(tool.known_folders().collect::<Vec<_>>(), [&workspace]);
}

#[test]
fn folders_are_working_directories_with_their_projects() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    // Bound project lookups to the fixture home and keep them off the shared global cache.
    let r = agents_builder(&scan)
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
    let r = agents_builder(&scan).with_home(None).build();
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
    let r = agents_builder(&scan).with_home(None).build();
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
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
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
    let inspected = Arc::new(Mutex::new(Vec::new()));
    let recorded = inspected.clone();
    let cache = crate::project::ProjectCache::new(
        Arc::new(move |cwd, home| {
            recorded.lock().unwrap().push(cwd.to_path_buf());
            crate::project::detect_project(cwd, home)
        }),
        Duration::from_secs(60),
    );
    let r = agents_builder(&scan)
        .with_project_cache(cache)
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
    let inspected = inspected.lock().unwrap();
    assert!(inspected.contains(&outside));
    assert!(
        !inspected.contains(&inside),
        "protected folders never reach the resolver"
    );
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
    let r = agents_builder(&scan)
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
fn recent_history_is_queried_only_for_confirmed_current_account_roots() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct RecordingRecent(AtomicUsize);
    impl RecentProjects for RecordingRecent {
        fn recent(&self, _: &str, _: &HashSet<PathBuf>) -> Vec<recent::RecentFolder> {
            self.0.fetch_add(1, Ordering::Relaxed);
            vec![recent::RecentFolder {
                path: "/history/current-account".into(),
                evidence: Evidence::Observed,
                note: "fixture history".into(),
            }]
        }
    }

    let cases = [
        (
            "linux",
            Some(1000),
            Some("dev"),
            vec![
                (100, Some(1000), None, true),
                (101, Some(1000), Some("renamed-user"), true),
                (200, Some(2000), Some("dev"), false),
                (300, None, None, false),
                (400, None, Some("dev"), true),
            ],
        ),
        (
            "windows",
            None,
            Some("work\\alice"),
            vec![
                (100, None, Some("work\\alice"), true),
                (200, None, Some("work\\bob"), false),
                (300, None, None, false),
                (400, None, Some(""), false),
            ],
        ),
        (
            "fixture",
            None,
            None,
            vec![
                (100, Some(1000), Some("dev"), false),
                (200, None, None, false),
            ],
        ),
        (
            "fixture",
            None,
            Some(""),
            vec![(100, None, Some(""), false)],
        ),
    ];
    for (platform, current_uid, current_user, roots) in cases {
        let mut current = proc(99, 1, "holdmap", &["holdmap"]);
        current.uid = current_uid;
        current.user = current_user.map(str::to_owned);
        let mut processes = vec![current];
        for (pid, uid, user, _) in &roots {
            let mut root = proc(*pid, 1, "claude", &["claude"]);
            root.uid = *uid;
            root.user = user.map(str::to_owned);
            root.cwd = Some(format!("/live/agent-{pid}").into());
            processes.push(root);
        }
        let scan = scan_of(processes, vec![], platform);
        let provider = RecordingRecent::default();
        let report = agents_builder(&scan)
            .with_home(None)
            .with_recent(&provider)
            .with_project_detection(false)
            .build();
        assert_eq!(
            provider.0.load(Ordering::Relaxed),
            roots.iter().filter(|(_, _, _, expected)| *expected).count(),
            "history must not be queried for other or unknown accounts on {platform}"
        );
        for (pid, _, _, expected) in roots {
            let agent = report.agents.iter().find(|agent| agent.pid == pid).unwrap();
            assert_eq!(
                agent.access.mine, expected,
                "account access must use the same UID-first identity as history for PID {pid} on {platform}"
            );
            let user_fact = agent
                .access
                .facts
                .iter()
                .find(|fact| fact.topic == AccessTopic::User)
                .unwrap();
            assert_eq!(
                user_fact.summary.starts_with("Runs as you ("),
                expected,
                "the account description must agree with access.mine for PID {pid} on {platform}"
            );
            assert_eq!(
                agent
                    .folders
                    .iter()
                    .any(|folder| folder.source == FolderSource::Recent),
                expected,
                "PID {pid} on {platform}"
            );
            assert!(
                agent
                    .folders
                    .iter()
                    .any(|folder| folder.source == FolderSource::Agent
                        && folder.path == format!("/live/agent-{pid}")),
                "live working directories remain visible for every account"
            );
        }
    }
}

#[test]
fn serialises_with_snake_case_tags() {
    let d = tempfile::tempdir().unwrap();
    let scan = machine(d.path());
    let r = agents_builder(&scan).with_home(None).build();
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
    let r = agents_builder(&scan).build();
    assert!(r.agents.is_empty());
    assert_eq!(r.platform, "linux");
}

#[test]
fn stdio_mcp_processes_are_visible_without_listening_ports() {
    let scan = scan_of(
        vec![
            proc(1, 0, "init", &[]),
            proc(200, 1, "claude", &["claude"]),
            proc(
                210,
                200,
                "node",
                &[
                    "node",
                    "/cache/node_modules/@modelcontextprotocol/server-filesystem/dist/index.js",
                    "--api-key=sk-live-secret",
                ],
            ),
            proc(211, 200, "uvx", &["uvx", "mcp-server-fetch"]),
            proc(212, 200, "bash", &["bash", "-c", "echo mcp-server-secret"]),
            proc(
                213,
                200,
                "cp",
                &["cp", "/tmp/mcp-server-secret", "/tmp/backup"],
            ),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![],
        "linux",
    );
    let report = agents_builder(&scan).with_project_detection(false).build();
    let agent = &report.agents[0];
    assert!(agent.ports.is_empty());
    let mcp: Vec<_> = agent
        .tools
        .iter()
        .filter(|t| t.kind == ToolKind::McpServer)
        .collect();
    assert_eq!(mcp.len(), 2);
    assert_eq!(mcp[0].name, "@modelcontextprotocol/server-filesystem");
    assert!(mcp
        .iter()
        .all(|t| t.evidence == Evidence::Inferred && t.ports.is_empty()));
    assert_eq!(
        agent.tools.iter().find(|t| t.pid == 212).unwrap().kind,
        ToolKind::Shell
    );
    assert_eq!(
        agent.tools.iter().find(|t| t.pid == 213).unwrap().kind,
        ToolKind::Command
    );
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("sk-live-secret"));
    assert!(!agent.matches("sk-live-secret"));
    assert!(
        !agent.matches("mcp-server-secret"),
        "tool argument text isn't searchable"
    );
    assert!(agent.matches("SERVER-FILESYSTEM"));
}

#[derive(Debug)]
struct CustomDetector;

impl AgentDetector for CustomDetector {
    fn identify(&self, process: &ProcessInfo) -> Option<&'static AgentProduct> {
        (process.name == "custom-runner").then(|| catalog::by_id("codex").unwrap())
    }
}

#[derive(Debug)]
struct CustomClassifier;

impl ToolClassifier for CustomClassifier {
    fn classify(&self, _: &ProcessInfo, _: &[AgentPort]) -> ToolClassification {
        ToolClassification {
            name: "custom-tool".into(),
            kind: ToolKind::Command,
            evidence: Evidence::Inferred,
        }
    }
}

#[test]
fn discovery_and_tool_classification_are_independently_injectable() {
    let scan = scan_of(
        vec![
            proc(200, 1, "custom-runner", &[]),
            proc(210, 200, "worker", &[]),
            proc(300, 1, "unrelated", &[]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![],
        "linux",
    );
    assert!(agents_builder(&scan).build().agents.is_empty());
    let report = agents_builder(&scan)
        .with_detector(&CustomDetector)
        .with_tool_classifier(&CustomClassifier)
        .with_project_detection(false)
        .build();
    assert_eq!(report.agents.len(), 1);
    assert_eq!(report.agents[0].process_ids, [200, 210]);
    assert_eq!(report.agents[0].tools[0].name, "custom-tool");
}

#[test]
fn clipped_lists_preserve_counts_child_priority_and_pid_search() {
    let mut procs = vec![
        proc(1, 0, "init", &[]),
        proc(100, 1, "Cursor", &["cursor"]),
        proc(99, 1, "holdmap", &[]),
    ];
    for pid in 101..151 {
        procs.push(proc(pid, 100, "Cursor Helper", &[]));
    }
    for pid in 200..230 {
        let cwd = PathBuf::from(format!("/workspace/project-{pid}"));
        procs.push(at(proc(pid, 100, "node", &["node", "task.js"]), &cwd));
    }
    let scan = scan_of(procs, vec![], "linux");
    let report = agents_builder(&scan).with_project_detection(false).build();
    let agent = &report.agents[0];
    assert_eq!(agent.processes.len(), MAX_PROCESSES);
    assert_eq!(agent.more_processes, 41);
    assert_eq!(agent.tools.len(), MAX_TOOLS);
    assert_eq!(agent.more_tools, 30 - MAX_TOOLS);
    assert_eq!(agent.folders.len(), MAX_FOLDERS);
    assert_eq!(agent.more_folders, 30 - MAX_FOLDERS);
    assert!(
        agent.processes.iter().any(|p| p.pid == 229),
        "children precede helpers"
    );
    assert!(!agent.processes.iter().any(|p| p.pid == 150));
    assert!(
        agent.matches("150"),
        "omitted helpers are still searchable by exact PID"
    );
    assert!(agent.matches("pid:229"));
    assert!(!agent.matches("15"));
    assert!(agent.matches("  "));
    assert!(agent.matches("aNySpHeRe"));
    assert!(agent.matches("PROJECT-200"));
}

#[test]
fn local_link_attribution_matches_bind_address_as_well_as_port() {
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(300, 1, "wrong-service", &[]),
            proc(301, 1, "right-service", &[]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![
            listen("127.0.0.1", 8000, 300),
            listen("127.0.0.2", 8000, 301),
            est(200, "127.0.0.1", 50000, "127.0.0.2", 8000),
        ],
        "linux",
    );
    let report = agents_builder(&scan).build();
    let link = &report.agents[0].links[0];
    assert_eq!(link.pid, Some(301));
    assert_eq!(link.process.as_deref(), Some("right-service"));
}

#[test]
fn remote_host_totals_include_omitted_links() {
    let raw = (1..=20)
        .map(|i| est(200, "10.0.0.1", 50000 + i, &format!("198.51.100.{i}"), 443))
        .collect();
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(99, 1, "holdmap", &[]),
        ],
        raw,
        "linux",
    );
    let report = agents_builder(&scan).build();
    let agent = &report.agents[0];
    assert_eq!(agent.links.len(), MAX_LINKS);
    assert_eq!(agent.more_links, 20 - MAX_LINKS);
    assert!(agent
        .access
        .fact(AccessTopic::Network)
        .unwrap()
        .summary
        .contains("20 remote hosts"));
}

#[test]
fn collection_warnings_and_inaccessible_metadata_reach_the_report() {
    let mut scan = scan_of(
        vec![proc(200, 1, "claude", &[]), proc(99, 1, "holdmap", &[])],
        vec![],
        "macos",
    );
    scan.snapshot
        .warnings
        .push("fixture provider failed".into());
    let report = agents_builder(&scan).build();
    assert!(report
        .limits
        .iter()
        .any(|l| l.contains("fixture provider failed")));
    assert!(report
        .limits
        .iter()
        .any(|l| l.contains("no readable command line")));
    assert!(report
        .limits
        .iter()
        .any(|l| l.contains("no readable working directory")));
    assert!(report.limits.iter().any(|l| l.contains("omit sockets")));
}

#[test]
fn old_agent_json_loads_without_new_visibility_fields() {
    let scan = scan_of(vec![proc(200, 1, "claude", &["claude"])], vec![], "linux");
    let report = agents_builder(&scan).build();
    let mut json = serde_json::to_value(&report.agents[0]).unwrap();
    for field in ["tools", "more_tools", "more_folders", "process_ids"] {
        json.as_object_mut().unwrap().remove(field);
    }
    let agent: Agent = serde_json::from_value(json).unwrap();
    assert!(agent.tools.is_empty());
    assert_eq!(agent.more_tools + agent.more_folders, 0);
    assert!(agent.matches("200"), "root PID still matches old payloads");
}

#[test]
fn every_child_holding_a_shared_listener_has_port_metadata() {
    let mut shared = listen("127.0.0.1", 5173, 210);
    shared.pids.push(211);
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(210, 200, "node", &["node", "node_modules/.bin/vite"]),
            proc(211, 200, "node", &["node", "node_modules/.bin/vite"]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![shared],
        "linux",
    );
    let report = agents_builder(&scan).build();
    let mut agent = report.agents[0].clone();
    assert_eq!(
        agent.ports.len(),
        1,
        "the parent list still represents one listener"
    );
    assert_eq!(agent.tools.len(), 2);
    assert!(agent
        .tools
        .iter()
        .all(|tool| tool.ports == [5173] && tool.kind == ToolKind::DevServer));
    agent.process_ids.clear();
    agent.processes.clear();
    assert!(
        agent.matches("211"),
        "tool PID remains a fallback for older or partial reports"
    );
}

#[test]
fn dual_stack_wildcards_correlate_ipv4_links_and_hide_accepted_connections() {
    let mut web = listen("::", 4000, 210);
    web.family = Family::V6;
    let mut database = listen("::", 5432, 300);
    database.family = Family::V6;
    let mut accepted = est(210, "::ffff:127.0.0.1", 4000, "::ffff:127.0.0.1", 50200);
    accepted.family = Family::V6;
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(210, 200, "node", &["node", "server.js"]),
            proc(300, 1, "postgres", &["postgres"]),
            proc(500, 1, "client", &[]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![
            web,
            database,
            accepted,
            est(210, "127.0.0.1", 50300, "127.0.0.1", 5432),
            est(300, "127.0.0.1", 5432, "127.0.0.1", 50300),
            est(500, "127.0.0.1", 50200, "127.0.0.1", 4000),
        ],
        "linux",
    );
    let report = agents_builder(&scan).build();
    let links = &report.agents[0].links;
    assert_eq!(
        links.len(),
        1,
        "the accepted connection is not an outbound dependency"
    );
    assert_eq!(links[0].pid, Some(300));
    assert_eq!(links[0].service.as_deref(), Some("PostgreSQL"));
    assert_eq!(links[0].connections, 1);
}

#[test]
fn ipv4_wildcards_win_over_possible_dual_stack_listeners() {
    let mut ipv6 = listen("::", 8000, 300);
    ipv6.family = Family::V6;
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(300, 1, "possible-dual-stack", &[]),
            proc(301, 1, "ipv4-listener", &[]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![
            ipv6,
            listen("0.0.0.0", 8000, 301),
            est(200, "127.0.0.1", 50000, "127.0.0.1", 8000),
        ],
        "linux",
    );
    let report = agents_builder(&scan).build();
    assert_eq!(report.agents[0].links[0].pid, Some(301));
}

#[test]
fn an_agent_port_names_its_owned_secondary_holder() {
    let mut socket = listen("127.0.0.1", 8000, 100);
    socket.pids.push(210);
    let scan = scan_of(
        vec![
            proc(200, 1, "claude", &["claude"]),
            proc(210, 200, "owned-worker", &[]),
            proc(100, 1, "outside-owner", &[]),
            proc(99, 1, "holdmap", &[]),
        ],
        vec![socket],
        "linux",
    );
    assert_eq!(
        scan.snapshot.entries[0].pid,
        Some(100),
        "fixture primary belongs outside the agent"
    );
    let report = agents_builder(&scan).build();
    let port = &report.agents[0].ports[0];
    assert_eq!(port.pid, Some(210));
    assert_eq!(port.process.as_deref(), Some("owned-worker"));
}
