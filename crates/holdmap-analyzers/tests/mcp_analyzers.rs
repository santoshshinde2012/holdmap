//! User-registered local MCP analyzers: registration, consent and the extra warning, the
#![recursion_limit = "256"]
//! runner (with an in-process fake connector: no server process is ever started) and the rmcp
//! client against an in-process fake MCP server over an in-memory pipe.

use holdmap_analyzers::finding::RunStatus;
use holdmap_analyzers::mcp_runner::{McpConnector, McpRunner, McpSession, ToolOutput};
use holdmap_analyzers::probe::{BinaryInfo, BinaryProbe};
use holdmap_analyzers::runner::{
    AnalyzerRunner, BasicSandbox, RawOutput, RunContext, RunError, Sandbox,
};
use holdmap_analyzers::secrets::Secret;
use holdmap_analyzers::service::{ManifestSource, ServiceError};
use holdmap_analyzers::{
    catalog, AnalyzerInput, AnalyzerManifest, AnalyzerService, AnalyzerState, FileStore, Stores,
};
use holdmap_core::agents::AgentsReport;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ID: &str = "example.mcp-scan";
const SARIF: &str = include_str!("fixtures/semgrep.sarif");

fn manifest_text() -> String {
    r#"schema = 1
id = "example.mcp-scan"
name = "Example MCP Scan"
publisher = "Example"
homepage = "https://example.com"
license = "MIT"
summary = "Scans a skill folder through a local MCP server."

[runner]
kind = "mcp"
command = "example-mcp"
args = ["--stdio"]
per_subject = "skill"
tool = "scan_skill"
timeout_secs = 30

[runner.tool_args]
path = "{subject.dir}"
format = "sarif"

[permissions]
read = ["{subject.dir}"]
spawns_mcp_servers = true
secrets = ["EXAMPLE_API_KEY"]

[output]
format = "sarif"
"#
    .to_string()
}

fn manifest() -> AnalyzerManifest {
    AnalyzerManifest::parse(&manifest_text()).unwrap()
}

#[test]
fn mcp_manifests_must_declare_the_server_and_pass_the_folder_in_tool_args() {
    assert!(AnalyzerManifest::parse(&manifest_text()).is_ok());
    let cases = [
        ("spawns_mcp_servers = true", "spawns_mcp_servers = false"),
        ("tool = \"scan_skill\"\n", ""),
        ("tool = \"scan_skill\"", "tool = \"scan skill; rm\""),
        ("path = \"{subject.dir}\"", "path = \"/etc\""),
        ("args = [\"--stdio\"]", "args = [\"{subject.dir}\"]"),
        ("command = \"example-mcp\"", "command = \"bash\""),
        (
            "format = \"sarif\"\n\n[permissions]",
            "format = \"{output}\"\n\n[permissions]",
        ),
    ];
    for (from, to) in cases {
        let text = manifest_text().replace(from, to);
        assert_ne!(text, manifest_text(), "{from}");
        assert!(AnalyzerManifest::parse(&text).is_err(), "accepted: {to}");
    }
    // A CLI analyzer can't claim to start MCP servers or name a tool.
    let cli =
        catalog::SOURCES[0].replace("spawns_mcp_servers = false", "spawns_mcp_servers = true");
    assert!(AnalyzerManifest::parse(&cli).is_err());
}

// ---------- runner with an in-process fake connector ----------

#[derive(Default)]
struct Log {
    connects: usize,
    closed: usize,
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    calls: Vec<(String, Map<String, Value>)>,
}

struct FakeConnector {
    log: Arc<Mutex<Log>>,
    tools: Vec<String>,
    reply: fn(&Map<String, Value>) -> Result<ToolOutput, String>,
}

struct FakeSession {
    log: Arc<Mutex<Log>>,
    tools: Vec<String>,
    reply: fn(&Map<String, Value>) -> Result<ToolOutput, String>,
}

impl McpConnector for FakeConnector {
    fn connect(
        &self,
        cmd: Command,
        _: &dyn Sandbox,
        _: Duration,
    ) -> Result<Box<dyn McpSession>, String> {
        let mut log = self.log.lock().unwrap();
        log.connects += 1;
        log.program = cmd.get_program().to_string_lossy().into_owned();
        log.args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        log.env = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().into_owned(),
                    v?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        Ok(Box::new(FakeSession {
            log: self.log.clone(),
            tools: self.tools.clone(),
            reply: self.reply,
        }))
    }
}

impl McpSession for FakeSession {
    fn tools(&self) -> Vec<String> {
        self.tools.clone()
    }
    fn call(
        &mut self,
        tool: &str,
        args: Map<String, Value>,
        _: Duration,
        _: usize,
    ) -> Result<ToolOutput, String> {
        self.log
            .lock()
            .unwrap()
            .calls
            .push((tool.into(), args.clone()));
        (self.reply)(&args)
    }
    fn close(self: Box<Self>) {
        self.log.lock().unwrap().closed += 1;
    }
}

fn sarif_reply(args: &Map<String, Value>) -> Result<ToolOutput, String> {
    let path = args["path"].as_str().unwrap();
    assert!(
        Path::new(path).join("SKILL.md").is_file(),
        "the staged copy exists during the call"
    );
    Ok(ToolOutput {
        bytes: SARIF.as_bytes().to_vec(),
        ..Default::default()
    })
}

fn input(dir: &Path) -> AnalyzerInput {
    serde_json::from_value(json!({
        "subjects": [{"subject": {"id": "skill:a", "kind": "skill", "name": "a", "agents": []}, "dir": dir, "package": null}],
        "skipped": []
    }))
    .unwrap()
}

fn skill(root: &Path) -> PathBuf {
    let dir = root.join("skill");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), "---\nname: a\n---\n").unwrap();
    dir
}

fn runner(
    log: &Arc<Mutex<Log>>,
    tools: &[&str],
    reply: fn(&Map<String, Value>) -> Result<ToolOutput, String>,
) -> McpRunner {
    McpRunner::new(
        Box::new(BasicSandbox),
        Box::new(FakeConnector {
            log: log.clone(),
            tools: tools.iter().map(|t| t.to_string()).collect(),
            reply,
        }),
    )
}

#[test]
fn the_server_starts_for_the_run_gets_a_cleared_env_and_staged_copies_and_is_stopped() {
    std::env::set_var("HOLDMAP_MCP_TEST_SECRET", "should-not-pass");
    let root = tempfile::tempdir().unwrap();
    let dir = skill(root.path());
    let log = Arc::new(Mutex::new(Log::default()));
    let r = runner(&log, &["scan_skill"], sarif_reply);
    assert_eq!(
        log.lock().unwrap().connects,
        0,
        "nothing starts before a run"
    );
    let cx = RunContext {
        binary: Some("/opt/bin/example-mcp".into()),
        secrets: vec![
            ("EXAMPLE_API_KEY".into(), Secret::new("k-1")),
            ("UNDECLARED".into(), Secret::new("nope")),
        ],
    };
    let out = r.run(&manifest(), &input(&dir), &cx).unwrap();
    assert_eq!(out.len(), 1);
    assert!(!out[0].failed, "{:?}", out[0].notes);
    let log = log.lock().unwrap();
    assert_eq!(
        (log.connects, log.closed),
        (1, 1),
        "started once, stopped after"
    );
    assert_eq!(log.program, "/opt/bin/example-mcp");
    assert_eq!(log.args, ["--stdio"]);
    let names: Vec<&str> = log.env.iter().map(|(k, _)| k.as_str()).collect();
    assert!(!names.contains(&"HOLDMAP_MCP_TEST_SECRET"), "{names:?}");
    assert!(!names.contains(&"UNDECLARED"));
    assert!(log.env.contains(&("EXAMPLE_API_KEY".into(), "k-1".into())));
    let (tool, args) = &log.calls[0];
    assert_eq!(tool, "scan_skill");
    assert_eq!(args["format"], "sarif");
    let staged = args["path"].as_str().unwrap();
    assert_ne!(
        Path::new(staged),
        dir.as_path(),
        "the original folder is never passed"
    );
    assert!(staged.contains("holdmap-analyzer-"));
    assert!(
        !Path::new(staged).exists(),
        "the staged copy is removed after the run"
    );
}

#[test]
fn a_missing_tool_a_tool_error_and_a_failed_start_are_reported_without_server_text() {
    let root = tempfile::tempdir().unwrap();
    let dir = skill(root.path());
    let cx = RunContext {
        binary: Some("/opt/bin/example-mcp".into()),
        ..Default::default()
    };

    let log = Arc::new(Mutex::new(Log::default()));
    let out = runner(&log, &["other"], sarif_reply)
        .run(&manifest(), &input(&dir), &cx)
        .unwrap();
    assert!(out[0].failed && out[0].notes[0].contains("doesn't offer the tool"));
    assert_eq!(
        log.lock().unwrap().closed,
        1,
        "stopped even when the tool is missing"
    );

    let log = Arc::new(Mutex::new(Log::default()));
    let out = runner(&log, &["scan_skill"], |_| {
        Ok(ToolOutput {
            bytes: b"secret ghp_LEAK in an error".to_vec(),
            is_error: true,
            truncated: false,
        })
    })
    .run(&manifest(), &input(&dir), &cx)
    .unwrap();
    assert!(out[0].failed);
    assert_eq!(out[0].notes, ["The tool reported an error."]);
    assert!(out[0].bytes.is_empty(), "error text is not kept");

    struct Refuses;
    impl McpConnector for Refuses {
        fn connect(
            &self,
            _: Command,
            _: &dyn Sandbox,
            _: Duration,
        ) -> Result<Box<dyn McpSession>, String> {
            Err("the program could not be started".into())
        }
    }
    let out = McpRunner::new(Box::new(BasicSandbox), Box::new(Refuses))
        .run(&manifest(), &input(&dir), &cx)
        .unwrap();
    assert_eq!(
        out[0].notes,
        ["The MCP server could not be started: the program could not be started."]
    );
}

// ---------- service: registration, consent, warning, never automatic ----------

struct Probe;
impl BinaryProbe for Probe {
    fn locate(&self, m: &AnalyzerManifest) -> Option<BinaryInfo> {
        Some(BinaryInfo {
            path: format!("/opt/bin/{}", m.runner.command.clone().unwrap_or_default()).into(),
            sha256: "b".repeat(64),
        })
    }
    fn version(&self, _: &AnalyzerManifest, _: &Path) -> Option<String> {
        None
    }
}

struct NoRun;
impl AnalyzerRunner for NoRun {
    fn run(
        &self,
        _: &AnalyzerManifest,
        _: &AnalyzerInput,
        _: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        Err(RunError::Unavailable("not in this test".into()))
    }
}

fn report(skill_dir: &Path, configured_package: &str) -> AgentsReport {
    serde_json::from_value(json!({
        "platform": "macos", "taken_at_ms": 1, "limits": [],
        "agents": [{
            "id": "agent:42", "product": "claude-code", "name": "Claude Code", "vendor": "Anthropic", "kind": "cli",
            "pid": 42, "process_name": "claude", "command": "claude", "started_at": 1,
            "parent": null, "memory_bytes": 1, "cpu_percent": 0.0, "processes": [], "more_processes": 0,
            "folders": [], "ports": [], "links": [], "more_links": 0,
            "access": {"user": "dev", "uid": 501, "root": false, "mine": true, "facts": []},
            "capabilities": {
                "inspected": true,
                "skills": [{"id": "skill:user:a", "name": "a", "scope": "user",
                            "source_path": skill_dir.join("SKILL.md"), "project_path": null, "evidence": "observed"}],
                "mcp_servers": [{
                    "id": "mcp:scanner", "name": "scanner", "scope": "user", "source_path": "/home/dev/.claude.json",
                    "project_path": null, "transport": "stdio", "enabled": true, "evidence": "observed",
                    "launch": {"runner": "binary", "package": configured_package, "pinned": null},
                    "env_names": [], "header_names": []
                }]
            }
        }]
    }))
    .unwrap()
}

fn service(store_dir: &Path, log: &Arc<Mutex<Log>>) -> AnalyzerService {
    AnalyzerService::with_parts(
        catalog::bundled(),
        Stores::of(FileStore::new(store_dir)),
        Box::new(Probe),
        Box::new(NoRun),
        Box::new(NoRun),
    )
    .with_mcp_runner(Box::new(runner(log, &["scan_skill"], sarif_reply)))
}

#[test]
fn a_registered_mcp_analyzer_needs_consent_warns_about_agent_servers_and_never_runs_on_its_own() {
    let root = tempfile::tempdir().unwrap();
    let dir = skill(root.path());
    let log = Arc::new(Mutex::new(Log::default()));
    let svc = service(&root.path().join("store"), &log);

    let view = svc.add_manifest(&manifest_text()).unwrap();
    assert_eq!(
        (view.state, view.source),
        (AnalyzerState::NotApproved, ManifestSource::User)
    );
    assert!(!view.background);
    assert_eq!(svc.list().len(), 3);

    // The same program as an agent's configured server: an extra warning.
    let r = report(&dir, "example-mcp");
    let p = svc.preview(ID, &r).unwrap();
    assert!(
        p.notices[0].contains("only when you run this analyzer"),
        "{:?}",
        p.notices
    );
    assert!(p
        .notices
        .iter()
        .any(|n| n.starts_with("Warning:") && n.contains("claude-code") && n.contains("scanner")));
    assert!(p
        .command
        .unwrap()
        .last()
        .unwrap()
        .contains("scan_skill(format=sarif, path=<copy of the skill folder>)"));
    assert!(p.schedule.starts_with("Only when you click Run now"));
    let other = svc.preview(ID, &report(&dir, "something-else")).unwrap();
    assert_eq!(other.notices.len(), 1, "no warning for an unrelated server");

    // Nothing runs before approval, and never automatically.
    assert!(matches!(
        svc.run(ID, &r),
        Err(ServiceError::Run(RunError::NotApproved(_)))
    ));
    svc.approve(ID).unwrap();
    assert!(svc.set_background(ID, true).is_err());
    assert!(!svc.run_changed(&r).contains(&ID.to_string()));
    assert_eq!(log.lock().unwrap().connects, 0);

    let result = svc.run(ID, &r).unwrap();
    assert_eq!(result.status, RunStatus::Complete);
    assert!(!result.findings.is_empty());
    let counts = {
        let l = log.lock().unwrap();
        (l.connects, l.closed)
    };
    assert_eq!(counts, (1, 1));

    // Editing the registered manifest asks again.
    let edited = manifest_text().replace("timeout_secs = 30", "timeout_secs = 60");
    svc.add_manifest(&edited).unwrap();
    assert_eq!(svc.get(ID).unwrap().state, AnalyzerState::Changed);

    assert!(svc.remove_manifest(ID).unwrap());
    assert!(svc.get(ID).is_err());
    assert!(
        svc.remove_manifest("nvidia.skillspector").is_err(),
        "catalog entries can't be removed"
    );
}

#[test]
fn registration_refuses_catalog_ids_built_ins_and_oversized_files() {
    let root = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Log::default()));
    let svc = service(&root.path().join("store"), &log);
    assert!(svc.add_manifest(catalog::SOURCES[0]).is_err(), "catalog id");
    assert!(svc.add_manifest(catalog::SOURCES[1]).is_err(), "built-in");
    let huge = format!("{}\n# {}", manifest_text(), "x".repeat(70 * 1024));
    assert!(svc.add_manifest(&huge).is_err());
    assert_eq!(svc.list().len(), 2);
}

// ---------- the rmcp client against an in-process fake server ----------

#[cfg(feature = "mcp-runner")]
mod rmcp_client {
    use super::*;
    use holdmap_analyzers::mcp_runner::RmcpSession;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    /// A minimal MCP server: answers initialize, tools/list and tools/call on an in-memory pipe.
    async fn fake_server(stream: tokio::io::DuplexStream) {
        let (read, mut write) = tokio::io::split(stream);
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let msg: Value = serde_json::from_str(&line).unwrap();
            let Some(id) = msg.get("id").cloned() else {
                continue;
            };
            let result = match msg["method"].as_str().unwrap_or("") {
                "initialize" => json!({
                    "protocolVersion": msg["params"]["protocolVersion"],
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "fake", "version": "1.2.3"}
                }),
                "tools/list" => {
                    json!({"tools": [{"name": "scan_skill", "inputSchema": {"type": "object"}}]})
                }
                "tools/call" => {
                    let path = msg["params"]["arguments"]["path"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let big = path.ends_with("big");
                    json!({
                        "content": [{"type": "text", "text": if big { "x".repeat(2048) } else { format!("{{\"scanned\":\"{path}\"}}") }}],
                        "isError": path.ends_with("broken")
                    })
                }
                _ => json!({}),
            };
            let reply = json!({"jsonrpc": "2.0", "id": id, "result": result});
            write
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
            write.flush().await.unwrap();
        }
    }

    #[test]
    fn the_rmcp_client_lists_tools_calls_them_and_caps_output() {
        let rt = RmcpSession::runtime().unwrap();
        let (client, server) = tokio::io::duplex(64 * 1024);
        rt.spawn(fake_server(server));
        let (read, write) = tokio::io::split(client);
        let mut session = RmcpSession::handshake(rt, read, write, Duration::from_secs(5)).unwrap();
        assert_eq!(session.tools(), ["scan_skill"]);
        let mut args = Map::new();
        args.insert("path".into(), json!("/tmp/staged/0"));
        let out = session
            .call("scan_skill", args.clone(), Duration::from_secs(5), 1024)
            .unwrap();
        assert_eq!(
            String::from_utf8(out.bytes).unwrap(),
            "{\"scanned\":\"/tmp/staged/0\"}"
        );
        assert!(!out.is_error);
        args.insert("path".into(), json!("/tmp/broken"));
        assert!(
            session
                .call("scan_skill", args.clone(), Duration::from_secs(5), 1024)
                .unwrap()
                .is_error
        );
        args.insert("path".into(), json!("/tmp/big"));
        let out = session
            .call("scan_skill", args, Duration::from_secs(5), 1024)
            .unwrap();
        assert!(out.truncated && out.bytes.is_empty());
        Box::new(session).close();
    }

    #[test]
    fn a_server_that_never_answers_times_out() {
        let rt = RmcpSession::runtime().unwrap();
        let (client, server) = tokio::io::duplex(1024);
        let (read, write) = tokio::io::split(client);
        let r = RmcpSession::handshake(rt, read, write, Duration::from_millis(200));
        assert!(r.is_err());
        drop(server);
    }
}
