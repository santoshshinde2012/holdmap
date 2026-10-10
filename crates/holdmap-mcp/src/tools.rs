//! Live tool execution delegates process ownership, classification and safety to core.

use holdmap_core::agents::{AgentsReport, Evidence, FolderSource};
use holdmap_core::topology::GraphExporter;
use holdmap_core::util::{count, human_bytes};
use holdmap_core::{
    ephemeral_port, port_busy, probe_tcp, tcp_accepting, Engine, Filter, ProbeResult, Protocol,
    ScanOptions, Snapshot, StopOptions, Target,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub(super) enum ToolError {
    Unknown,
    Failed(String),
}

pub(super) fn port_arg(args: &Value, key: &str) -> Result<u16, ToolError> {
    args[key]
        .as_f64()
        .filter(|p| p.fract() == 0.0 && (1.0..=65535.0).contains(p))
        .map(|p| p as u16)
        .ok_or_else(|| ToolError::Failed(format!("`{key}` must be an integer between 1 and 65535")))
}

pub(super) fn engine() -> Result<Engine, ToolError> {
    Engine::new(&ScanOptions::default()).map_err(|e| ToolError::Failed(format!("scan failed: {e}")))
}

/// Preserve scan quality and freshness while presenting filtered listener rows.
pub(super) fn ports_result(snapshot: &Snapshot, args: &Value) -> (String, Value) {
    let filter = Filter {
        query: args["query"].as_str().unwrap_or("").to_string(),
        dev_only: args["dev_only"].as_bool().unwrap_or(false),
        protocol: match args["protocol"].as_str() {
            Some("tcp") => Some(Protocol::Tcp),
            Some("udp") => Some(Protocol::Udp),
            _ => None,
        },
        listening_only: true,
        ..Default::default()
    };
    let shown = filter.apply(&snapshot.entries);
    let hidden = shown.iter().filter(|x| x.is_hidden()).count();
    let rows: Vec<Value> = shown
        .into_iter()
        .map(|x| {
            json!({
                "port": x.port, "protocol": x.protocol, "addresses": x.addresses, "pid": x.pid,
                "process": x.process.as_ref().map(|p| p.name.clone()),
                "command": x.process.as_ref().map(|p| p.command()),
                "label": x.label, "framework": x.framework.as_ref().map(|f| f.name.clone()),
                "project": x.project.as_ref().map(|p| json!({"name": p.name, "root": p.root, "branch": p.git_branch})),
                "container": x.container.as_ref().map(|c| json!({"name": c.name, "image": c.image})),
                "is_dev": x.is_dev, "exposure": x.exposure, "protected": x.protected
            })
        })
        .collect();
    let mut lines = vec![format!("{} in use.", count(rows.len(), "port", "ports"))];
    lines.extend(
        snapshot
            .warnings
            .iter()
            .map(|warning| format!("Warning: {warning}")),
    );
    (
        lines.join("\n"),
        json!({
            "ports": rows, "hidden_sockets": hidden,
            "warnings": snapshot.warnings, "platform": snapshot.platform,
            "taken_at_ms": snapshot.taken_at_ms
        }),
    )
}

/// Present a core report without reconstructing ownership or tool classification in the adapter.
pub(super) fn agents_result(mut report: AgentsReport, args: &Value) -> (String, Value) {
    if let Some(query) = args["agent"].as_str() {
        report.agents.retain(|agent| agent.matches(query));
    }
    let mut lines = Vec::new();
    for agent in &report.agents {
        let folders: Vec<&str> = agent
            .folders
            .iter()
            .filter(|folder| folder.source != FolderSource::Recent)
            .map(|folder| folder.label.as_str())
            .collect();
        lines.push(format!(
            "{} (pid {}): {}; {}; {}; {}; {}; {:.1}% CPU; {}",
            agent.name,
            agent.pid,
            if folders.is_empty() {
                "no working folders visible".into()
            } else {
                format!("works in {}", folders.join(", "))
            },
            count(agent.tools.len() + agent.more_tools, "tool", "tools"),
            count(agent.ports.len(), "port", "ports"),
            count(
                agent.links.len() + agent.more_links,
                "connection target",
                "connection targets"
            ),
            count(
                agent.processes.len() + agent.more_processes,
                "process",
                "processes"
            ),
            agent.cpu_percent,
            human_bytes(agent.memory_bytes),
        ));
        lines.push(format!(
            "  {} · {}",
            agent.kind.label(),
            count(
                agent.stoppable_ports().count(),
                "stoppable port",
                "stoppable ports"
            )
        ));
        if let Some(host) = agent
            .parent
            .as_ref()
            .and_then(|parent| report.agents.iter().find(|host| &host.id == parent))
        {
            lines.push(format!("  Started from {} (pid {}).", host.name, host.pid));
        }
        for fact in &agent.access.facts {
            lines.push(format!("  Access: {}", fact.summary));
        }
        for tool in &agent.tools {
            let evidence = match tool.evidence {
                Evidence::Observed => "observed",
                Evidence::Inferred => "inferred",
                Evidence::Unknown => "unknown",
            };
            let ports = if tool.ports.is_empty() {
                "no listening ports".to_string()
            } else {
                tool.ports
                    .iter()
                    .map(|port| format!(":{port}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            lines.push(format!(
                "  {} (pid {}): {} · {evidence} · {ports}",
                tool.name,
                tool.pid,
                tool.kind.label()
            ));
        }
        let omitted = [
            (agent.more_tools, "tool", "tools"),
            (agent.more_folders, "folder", "folders"),
            (agent.more_processes, "process", "processes"),
            (agent.more_links, "connection target", "connection targets"),
        ]
        .into_iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, one, many)| count(n, one, many))
        .collect::<Vec<_>>();
        if !omitted.is_empty() {
            lines.push(format!("  Not listed: {}.", omitted.join(", ")));
        }
    }
    if lines.is_empty() {
        lines.push(
            match args["agent"].as_str().filter(|q| !q.trim().is_empty()) {
                Some(query) => format!("No agents or developer tools match {query:?}."),
                None => "No agents or developer tools running.".to_string(),
            },
        );
    }
    lines.extend(report.limits.iter().map(|limit| format!("Note: {limit}")));
    let text = lines.join("\n");
    (text, serde_json::to_value(&report).unwrap_or_default())
}

pub(super) fn call(
    name: &str,
    args: &Value,
    confirmations: &mut crate::confirmation::Confirmations,
) -> Result<(String, Value), ToolError> {
    if matches!(name, "stop_port" | "stop_agent_ports") {
        return crate::stops::call(name, args, confirmations);
    }
    validate_args(name, args)?;
    match name {
        "list_ports" => Ok(ports_result(engine()?.snapshot(), args)),
        "get_topology" => {
            let e = engine()?;
            let mut g = e.topology();
            if !args["all"].as_bool().unwrap_or(false) {
                g.retain_dev();
            }
            let format = args["format"].as_str().unwrap_or("json");
            let exporter = holdmap_core::topology::exporter(format)
                .ok_or_else(|| ToolError::Failed(format!("unknown format `{format}`")))?;
            let text = if format == "json" {
                holdmap_core::topology::TreeExporter::ascii().export(&g)
            } else {
                exporter.export(&g)
            };
            Ok((text, serde_json::to_value(&g).unwrap_or_default()))
        }
        "list_agents" => Ok(agents_result(engine()?.agents(), args)),
        "plan_cluster_stop" => {
            let c = args["cluster"]
                .as_str()
                .filter(|c| !c.trim().is_empty())
                .ok_or_else(|| ToolError::Failed("`cluster` is required".into()))?;
            let plan = engine()?.plan(&Target::Cluster(c.to_string()), &StopOptions::default());
            let mut text = plan.summary.clone();
            for w in &plan.warnings {
                text.push_str(&format!("\nWarning: {w}"));
            }
            Ok((text, serde_json::to_value(&plan).unwrap_or_default()))
        }
        "explain_port" => {
            let port = port_arg(args, "port")?;
            // Connected sockets too, so the answer can say who is talking to the port.
            let e = Engine::new(&ScanOptions {
                all_states: true,
                ..ScanOptions::default()
            })
            .map_err(|e| ToolError::Failed(format!("scan failed: {e}")))?;
            let ex = e.explain(port, &StopOptions::default());
            let details = holdmap_core::details::for_port(&e.scan, port);
            let http = ex
                .entries
                .iter()
                .any(|x| x.protocol == holdmap_core::model::Protocol::Tcp && x.state.is_listening())
                .then(|| holdmap_core::http::probe(port, "/", Duration::from_millis(800)))
                .flatten();
            let mut text = format!(
                "{}\n{}\nRecommendation: {}",
                ex.headline,
                ex.details.join("\n"),
                ex.recommendation
            );
            for d in &details {
                let c = &d.connections;
                if c.total > 0 {
                    let peers: Vec<String> = c
                        .peers
                        .iter()
                        .map(|p| {
                            format!(
                                "{} ×{}",
                                p.process.as_deref().unwrap_or(&p.address),
                                p.connections
                            )
                        })
                        .collect();
                    text.push_str(&format!(
                        "\nConnections: {} ({} established): {}",
                        c.total,
                        c.established,
                        peers.join(", ")
                    ));
                }
                text.push_str(&format!(
                    "\nBind risk ({:?}): {}. {}",
                    d.bind_risk.level, d.bind_risk.title, d.bind_risk.explanation
                ));
            }
            if let Some(h) = &http {
                text.push_str(&format!("\nHTTP: {}", h.summary()));
            }
            let mut v = serde_json::to_value(&ex).unwrap_or_default();
            v["details"] = serde_json::to_value(&details).unwrap_or_default();
            v["http"] = serde_json::to_value(&http).unwrap_or_default();
            Ok((text, v))
        }
        "find_free_port" => {
            let port = match args.get("near") {
                Some(_) => (port_arg(args, "near")?..=65535)
                    .take(500)
                    .find(|p| probe_tcp(*p) == ProbeResult::Free)
                    .ok_or_else(|| {
                        ToolError::Failed("no free port found near the requested one".into())
                    })?,
                None => ephemeral_port().map_err(|e| ToolError::Failed(e.to_string()))?,
            };
            Ok((format!("Port {port} is free."), json!({"port": port})))
        }
        "wait_for_port" => {
            let port = port_arg(args, "port")?;
            let timeout = Duration::from_secs_f64(args["timeout_s"].as_f64().unwrap_or(30.0));
            let until_free = args["until_free"].as_bool().unwrap_or(false);
            let started = Instant::now();
            loop {
                let done = if until_free {
                    !port_busy(port, Protocol::Tcp)
                } else {
                    tcp_accepting(port)
                };
                if done {
                    let ms = started.elapsed().as_millis() as u64;
                    let what = if until_free {
                        "is free"
                    } else {
                        "is accepting connections"
                    };
                    return Ok((
                        format!("Port {port} {what} (after {ms} ms)."),
                        json!({"port": port, "ready": true, "elapsed_ms": ms}),
                    ));
                }
                if started.elapsed() >= timeout {
                    return Err(ToolError::Failed(format!(
                        "Timed out after {:.1}s waiting for port {port}.",
                        timeout.as_secs_f64()
                    )));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        _ => Err(ToolError::Unknown),
    }
}

pub(super) fn validate_args(name: &str, args: &Value) -> Result<(), ToolError> {
    let definition = super::catalog::definition(name).ok_or(ToolError::Unknown)?;
    super::validation::validate(args, &definition["inputSchema"]).map_err(ToolError::Failed)
}
