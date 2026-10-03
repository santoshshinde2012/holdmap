#![warn(missing_docs)]
//! # portwise-mcp
//!
//! A dependency-light [Model Context Protocol](https://modelcontextprotocol.io) server over stdio
//! (newline-delimited JSON-RPC 2.0) that lets AI coding agents list ports, ask *why* a port is
//! busy, find a free port, wait for a port, and stop a port — under a strict safety policy:
//!
//! * `stop_port` only acts on **low-risk** owners (your own dev servers and containers) unless
//!   the agent passes `allow_non_dev: true`, and never on protected or other users' processes.
//! * `dry_run: true` returns the plan without executing it.

use portwise_core::topology::GraphExporter;
use portwise_core::{
    ephemeral_port, execute, port_busy, probe_tcp, tcp_accepting, Engine, Filter, ProbeResult,
    Protocol, Risk, ScanOptions, StopOptions, Target,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Run the server until stdin closes.
pub fn serve<R: BufRead, W: Write>(input: R, mut output: W) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch.into_iter().filter_map(handle).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => handle(msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = response {
            writeln!(output, "{}", serde_json::to_string(&r)?)?;
            output.flush()?;
        }
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Handle one JSON-RPC message; returns `None` for notifications.
pub fn handle(msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let id = id?; // notifications (no id) get no response
    Some(match method {
        "initialize" => {
            let requested = params["protocolVersion"]
                .as_str()
                .unwrap_or(SUPPORTED_VERSIONS[0]);
            let version = if SUPPORTED_VERSIONS.contains(&requested) {
                requested
            } else {
                SUPPORTED_VERSIONS[0]
            };
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "portwise", "title": "portwise", "version": portwise_core::VERSION},
                    "instructions": "Use explain_port before stopping anything. Prefer find_free_port over stopping processes you didn't start. stop_port only stops the user's own dev servers/containers by default."
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args) {
                Ok((text, data)) => ok(
                    id,
                    json!({
                        "content": [{"type": "text", "text": format!("{text}\n\n{}", serde_json::to_string_pretty(&data).unwrap_or_default())}],
                        "structuredContent": data,
                        "isError": false
                    }),
                ),
                Err(ToolError::Unknown) => error(id, -32602, &format!("unknown tool: {name}")),
                Err(ToolError::Failed(msg)) => ok(
                    id,
                    json!({
                        "content": [{"type": "text", "text": msg}],
                        "isError": true
                    }),
                ),
            }
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "list_ports",
            "title": "List ports in use",
            "description": "List listening TCP/UDP ports with owning process, PID, project and framework labels.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "Filter, e.g. 'vite', ':3000', '3000-3999', 'proto:udp'"},
                "dev_only": {"type": "boolean", "description": "Only likely development servers"},
                "protocol": {"type": "string", "enum": ["tcp", "udp"]}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "explain_port",
            "title": "Explain why a port is busy",
            "description": "Plain-English root cause for a port (process tree, container, supervisor, OS service, TIME_WAIT, other user) plus the recommended fix and stop plan.",
            "inputSchema": {"type": "object", "properties": {"port": {"type": "integer", "minimum": 1, "maximum": 65535}}, "required": ["port"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "find_free_port",
            "title": "Find a free port",
            "description": "Return a free TCP port, optionally the first free one at or after `near`.",
            "inputSchema": {"type": "object", "properties": {"near": {"type": "integer", "minimum": 1, "maximum": 65535}}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "wait_for_port",
            "title": "Wait for a port",
            "description": "Block until something accepts connections on localhost:port (or until it is free with `until_free`).",
            "inputSchema": {"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "timeout_s": {"type": "number", "default": 30},
                "until_free": {"type": "boolean", "default": false}
            }, "required": ["port"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "get_topology",
            "title": "Service dependency graph",
            "description": "Which local services talk to which: nodes (services, containers, clients, external hosts), edges (live TCP connections; `a -> b` means a depends on b) and clusters (compose project, k8s namespace, supervisor such as turbo/pm2/concurrently, monorepo workspace, git repo). Use before stopping something to see what depends on it.",
            "inputSchema": {"type": "object", "properties": {
                "all": {"type": "boolean", "default": false, "description": "Include system/app services, not only dev services and their peers"},
                "format": {"type": "string", "enum": ["json", "mermaid", "dot", "tree"], "default": "json"}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "plan_cluster_stop",
            "title": "Plan stopping a cluster",
            "description": "Dry-run only: the dependency-ordered plan (dependents first) for stopping every service in a cluster, with warnings about outside dependents. Never executes; ask the user to run `portwise stop --cluster NAME`.",
            "inputSchema": {"type": "object", "properties": {"cluster": {"type": "string"}}, "required": ["cluster"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_port",
            "title": "Stop whatever holds a port",
            "description": "Safely stop the owner of a port (graceful tree stop, container stop or supervisor command) and verify the port is free. By default only stops the user's own dev servers/containers; protected and other users' processes are always refused.",
            "inputSchema": {"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "dry_run": {"type": "boolean", "default": false, "description": "Only return the plan"},
                "force": {"type": "boolean", "default": false, "description": "SIGKILL immediately"},
                "allow_non_dev": {"type": "boolean", "default": false, "description": "Allow stopping a non-dev process of the user"}
            }, "required": ["port"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false}
        }
    ])
}

enum ToolError {
    Unknown,
    Failed(String),
}

fn port_arg(args: &Value, key: &str) -> Result<u16, ToolError> {
    args[key]
        .as_u64()
        .filter(|p| (1..=65535).contains(p))
        .map(|p| p as u16)
        .ok_or_else(|| ToolError::Failed(format!("`{key}` must be an integer between 1 and 65535")))
}

fn engine() -> Result<Engine, ToolError> {
    Engine::new(&ScanOptions::default()).map_err(|e| ToolError::Failed(format!("scan failed: {e}")))
}

fn call_tool(name: &str, args: &Value) -> Result<(String, Value), ToolError> {
    match name {
        "list_ports" => {
            let e = engine()?;
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
            let rows: Vec<Value> = filter
                .apply(&e.snapshot().entries)
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
            Ok((
                format!("{} port(s) in use.", rows.len()),
                json!({"ports": rows, "hidden_sockets": e.snapshot().hidden_sockets}),
            ))
        }
        "get_topology" => {
            let e = engine()?;
            let mut g = e.topology();
            if !args["all"].as_bool().unwrap_or(false) {
                g.retain_dev();
            }
            let format = args["format"].as_str().unwrap_or("json");
            let exporter = portwise_core::topology::exporter(format)
                .ok_or_else(|| ToolError::Failed(format!("unknown format `{format}`")))?;
            let text = if format == "json" {
                portwise_core::topology::TreeExporter::ascii().export(&g)
            } else {
                exporter.export(&g)
            };
            Ok((text, serde_json::to_value(&g).unwrap_or_default()))
        }
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
            let ex = engine()?.explain(port, &StopOptions::default());
            let text = format!(
                "{}\n{}\nRecommendation: {}",
                ex.headline,
                ex.details.join("\n"),
                ex.recommendation
            );
            Ok((text, serde_json::to_value(&ex).unwrap_or_default()))
        }
        "find_free_port" => {
            let port = match args["near"].as_u64() {
                Some(n) if (1..=65535).contains(&n) => (n as u16..=65535)
                    .take(500)
                    .find(|p| probe_tcp(*p) == ProbeResult::Free)
                    .ok_or_else(|| {
                        ToolError::Failed("no free port found near the requested one".into())
                    })?,
                _ => ephemeral_port().map_err(|e| ToolError::Failed(e.to_string()))?,
            };
            Ok((format!("Port {port} is free."), json!({"port": port})))
        }
        "wait_for_port" => {
            let port = port_arg(args, "port")?;
            let timeout = Duration::from_secs_f64(
                args["timeout_s"].as_f64().unwrap_or(30.0).clamp(0.1, 600.0),
            );
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
        "stop_port" => {
            let port = port_arg(args, "port")?;
            let opts = StopOptions {
                force: args["force"].as_bool().unwrap_or(false),
                ..Default::default()
            };
            let e = engine()?;
            let plan = e.plan(&Target::Port(port), &opts);
            if let Some(b) = &plan.blocked {
                return Err(ToolError::Failed(format!("Refused: {}", b.message)));
            }
            let allow_non_dev = args["allow_non_dev"].as_bool().unwrap_or(false);
            if plan.risk == Risk::High || (plan.risk == Risk::Medium && !allow_non_dev) {
                return Err(ToolError::Failed(format!(
                    "Refused by agent safety policy (risk: {:?}). {} Ask the user to run `portwise stop {port}` themselves{}.",
                    plan.risk,
                    plan.warnings.join(" "),
                    if plan.risk == Risk::Medium { ", or retry with allow_non_dev=true if they asked you to" } else { "" }
                )));
            }
            if args["dry_run"].as_bool().unwrap_or(false) {
                let steps: Vec<String> = plan.steps.iter().map(|s| s.describe()).collect();
                return Ok((
                    format!("Plan (dry run): {}\n- {}", plan.summary, steps.join("\n- ")),
                    serde_json::to_value(&plan).unwrap_or_default(),
                ));
            }
            let report = execute(&plan, &mut |_| {});
            if report.success {
                Ok((
                    format!("Port {port} is free (took {} ms).", report.elapsed_ms),
                    serde_json::to_value(&report).unwrap_or_default(),
                ))
            } else {
                Err(ToolError::Failed(format!(
                    "Stop failed: {}\n{}",
                    report.error.unwrap_or_default(),
                    report.log.join("\n")
                )))
            }
        }
        _ => Err(ToolError::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(msg: Value) -> Value {
        handle(msg).expect("response")
    }

    #[test]
    fn initialize_negotiates_version() {
        let r = call(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "portwise");
        let r = call(
            json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], SUPPORTED_VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_response() {
        assert!(handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    }

    #[test]
    fn lists_tools_with_annotations() {
        let r = call(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}));
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        let stop = tools.iter().find(|t| t["name"] == "stop_port").unwrap();
        assert_eq!(stop["annotations"]["destructiveHint"], true);
    }

    #[test]
    fn unknown_method_and_tool() {
        assert_eq!(
            call(json!({"jsonrpc":"2.0","id":4,"method":"nope"}))["error"]["code"],
            -32601
        );
        assert_eq!(
            call(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}))
                ["error"]["code"],
            -32602
        );
    }

    #[test]
    fn find_free_port_and_explain() {
        let r = call(
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"find_free_port","arguments":{}}}),
        );
        let port = r["result"]["structuredContent"]["port"].as_u64().unwrap();
        assert!(port > 0);
        let r = call(
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"explain_port","arguments":{"port":port}}}),
        );
        assert_eq!(r["result"]["structuredContent"]["status"], "free");
        let r = call(
            json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"explain_port","arguments":{"port":0}}}),
        );
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn refuses_to_stop_own_test_process() {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let r = call(
            json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":port}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Refused"));
    }

    #[test]
    fn serve_loop_round_trip() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\nnot json\n";
        let mut out = Vec::new();
        serve(&input[..], &mut out).unwrap();
        let lines: Vec<&str> = std::str::from_utf8(&out).unwrap().lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"result\":{}"));
        assert!(lines[1].contains("-32700"));
    }

    #[test]
    fn topology_and_cluster_plan_are_read_only() {
        let tools = tools();
        for name in ["get_topology", "plan_cluster_stop"] {
            let t = tools
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == name)
                .unwrap();
            assert_eq!(t["annotations"]["readOnlyHint"], true, "{name}");
        }
        let r = call(
            json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"get_topology","arguments":{"all":true,"format":"mermaid"}}}),
        );
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("flowchart LR"));
        assert!(r["result"]["structuredContent"]["nodes"].is_array());
        let r = call(
            json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"plan_cluster_stop","arguments":{"cluster":"nope"}}}),
        );
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("No cluster named"));
    }
}
