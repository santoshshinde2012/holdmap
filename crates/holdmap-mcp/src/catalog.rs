//! Static tool discovery metadata and the input/output contracts shared by the adapter.

use serde_json::{json, Value};

pub(super) const INSTRUCTIONS: &str = "Inspect local state before acting: list_ports finds occupied ports; explain_port explains a conflict; get_topology with all=true shows dependents and cluster names before a stop. Prefer find_free_port to stopping an unrelated process, and use wait_for_port after starting a service. list_agents shows observed process ownership, likely child tools and access evidence, not chats or confirmed MCP invocations; inspect limits and omitted counts. Read holdmap://guide for workflows, or use the diagnose_port, inspect_agents and prepare_dev_server prompts. stop_agent_ports previews only unprotected services started by a specific matching agent by default. Before stop_port or executing stop_agent_ports, show its dry_run plan and get the user's OK. Only the user's own dev servers/containers are stopped by default; protected and other users' processes are always refused.";

pub(super) fn list() -> Value {
    let mut tools = json!([
        {
            "name": "list_ports",
            "title": "List ports in use",
            "description": "Inspect listening TCP/UDP ports before starting a service. Returns owners, PIDs, projects, frameworks, exposure and protection flags; hidden_sockets counts rows whose owner is unavailable. Includes platform, taken_at_ms (Unix epoch milliseconds) and scan warnings: check warnings before treating an empty result as complete. Use explain_port for a busy port's cause and safe next step. Commands have secrets hidden.",
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
            "description": "Plain-English root cause for a port (process tree, container, supervisor, OS service, TIME_WAIT, other user) plus the recommended fix and stop plan, who is connected, uptime, bind-address risk and the HTTP status/title. Command lines have secrets hidden.",
            "inputSchema": {"type": "object", "properties": {"port": {"type": "integer", "minimum": 1, "maximum": 65535}}, "required": ["port"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "find_free_port",
            "title": "Find a free port",
            "description": "Find a free TCP port before starting a server. With near, checks at most 500 ports at or above that number; without it, asks the OS for an ephemeral port. The result is a point-in-time check and does not reserve the port.",
            "inputSchema": {"type": "object", "properties": {"near": {"type": "integer", "minimum": 1, "maximum": 65535}}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "wait_for_port",
            "title": "Wait for a port",
            "description": "Wait for TCP acceptance on localhost:port, or for the port to be free with until_free. This checks socket readiness, not HTTP/application health. Blocks this stdio server until ready or timeout; timeout_s is 0.1–600 seconds (default 30).",
            "inputSchema": {"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "timeout_s": {"type": "number", "minimum": 0.1, "maximum": 600, "default": 30},
                "until_free": {"type": "boolean", "default": false}
            }, "required": ["port"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "get_topology",
            "title": "Service dependency graph",
            "description": "Which local services talk to which: nodes (services, containers, clients, external hosts), edges (live TCP connections; `a -> b` means a depends on b) and clusters (compose project, k8s namespace, supervisor such as turbo/pm2/concurrently, monorepo workspace, git repo). Use all=true before stopping something to include system/app dependents; the default view is filtered to development services and peers.",
            "inputSchema": {"type": "object", "properties": {
                "all": {"type": "boolean", "default": false, "description": "Include system/app services, not only dev services and their peers"},
                "format": {"type": "string", "enum": ["json", "mermaid", "dot", "tree"], "default": "json"}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "list_agents",
            "title": "Agents and developer tools on this machine",
            "description": "Inspect running AI coding agents (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider…) and developer tools (Docker Desktop, OrbStack…) with their working folders, child tools, listening ports, local services, remote IPs and CPU/memory. Likely MCP server processes include stdio-only servers; this does not confirm a protocol handshake or individual tool invocations. Classification/access evidence is observed, inferred or unknown. Recent folders do not prove current access. process_ids includes all owned PIDs; displayed lists include omitted counts. Consult limits for collection gaps. Commands have secrets hidden; chats, settings and credentials are never read.",
            "inputSchema": {"type": "object", "properties": {
                "agent": {"type": "string", "description": "Filter by product id, name, vendor, folder, tool name or exact agent/child PID, e.g. 'claude', 'shop-web', 'filesystem-mcp' or '1234'"}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_agent_ports",
            "title": "Stop ports an agent started",
            "description": "Plan or safely stop the unprotected development servers and services started by a specific matching agent or developer tool; its own IDE, auth and protected listeners remain refused. Defaults to dry_run=true. Before executing with dry_run=false, inspect get_topology with all=true, show the plan and get the user's OK. Only low-risk plans execute; protected or other users' processes and medium/high-risk plans are refused. Prefer an exact PID or a specific agent filter.",
            "inputSchema": {"type": "object", "properties": {
                "agent": {"type": "string", "minLength": 1, "description": "Match product, name, vendor, folder, tool name or exact root/owned-child PID; use a specific match"},
                "dry_run": {"type": "boolean", "default": true, "description": "Only return plans; set false only after user authorization"},
                "force": {"type": "boolean", "default": false, "description": "SIGKILL immediately"}
            }, "required": ["agent"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false}
        },
        {
            "name": "plan_cluster_stop",
            "title": "Plan stopping a cluster",
            "description": "Dry-run only: the dependency-ordered plan (dependents first) for stopping every service in a cluster, with warnings about outside dependents. Never executes; ask the user to run `holdmap stop --cluster NAME`.",
            "inputSchema": {"type": "object", "properties": {"cluster": {"type": "string", "minLength": 1, "description": "Cluster id or name from get_topology"}}, "required": ["cluster"]},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_port",
            "title": "Stop whatever holds a port",
            "description": "Safely stop the owner of a port (graceful tree stop, container stop or supervisor command) and verify the port is free. Destructive: show the user the plan (dry_run=true) and get their OK first. By default only stops the user's own dev servers/containers; protected processes (system, IDE, terminal, the agent's own session) and other users' processes are always refused, with no override.",
            "inputSchema": {"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "dry_run": {"type": "boolean", "default": false, "description": "Only return the plan"},
                "force": {"type": "boolean", "default": false, "description": "SIGKILL immediately"},
                "allow_non_dev": {"type": "boolean", "default": false, "description": "Allow stopping a non-dev process of the user"}
            }, "required": ["port"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false}
        }
    ]);
    for tool in tools
        .as_array_mut()
        .expect("static tool catalog is an array")
    {
        tool["inputSchema"]["additionalProperties"] = json!(false);
        tool["outputSchema"] = output_schema(tool["name"].as_str().expect("static tool name"));
    }
    tools
}

pub(super) fn definition(name: &str) -> Option<Value> {
    list()
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
        .cloned()
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required})
}

fn array(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn strings() -> Value {
    array(json!({"type": "string"}))
}

fn count() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn port() -> Value {
    json!({"type": "integer", "minimum": 1, "maximum": 65535})
}

fn plan_schema() -> Value {
    object(
        json!({
            "target": {"type": "string"}, "summary": {"type": "string"},
            "owners": {"type": "array", "items": {"type": "object"}},
            "steps": {"type": "array", "items": {"type": "object"}},
            "blocked": {"type": ["object", "null"]}, "warnings": strings(),
            "risk": {"type": "string", "enum": ["low", "medium", "high"]}
        }),
        &[
            "target", "summary", "owners", "steps", "blocked", "warnings", "risk",
        ],
    )
}

fn stop_report_schema() -> Value {
    object(
        json!({
            "target": {"type": "string"}, "success": {"type": "boolean"}, "freed": {"type": "boolean"},
            "ports_still_busy": array(port()), "signalled": array(count()), "escalated": {"type": "boolean"},
            "survivors": array(count()), "elapsed_ms": count(), "log": strings(), "error": {"type": ["string", "null"]}
        }),
        &[
            "target",
            "success",
            "freed",
            "ports_still_busy",
            "signalled",
            "escalated",
            "survivors",
            "elapsed_ms",
            "log",
            "error",
        ],
    )
}

fn agents_schema() -> Value {
    let tool = object(
        json!({
            "pid": count(), "ppid": {"type": ["integer", "null"]},
            "name": {"type": "string"}, "command": {"type": "string"},
            "kind": {"type": "string", "enum": ["mcp_server", "dev_server", "shell", "command"]},
            "cwd": {"type": ["string", "null"]},
            "evidence": {"type": "string", "enum": ["observed", "inferred", "unknown"]},
            "ports": array(port()), "memory_bytes": count(), "cpu_percent": {"type": "number"}
        }),
        &[
            "pid",
            "ppid",
            "name",
            "command",
            "kind",
            "cwd",
            "evidence",
            "ports",
            "memory_bytes",
            "cpu_percent",
        ],
    );
    let agent = object(
        json!({
            "id": {"type": "string"}, "product": {"type": "string"},
            "name": {"type": "string"}, "vendor": {"type": "string"}, "pid": count(),
            "kind": {"type": "string", "enum": ["cli", "ide", "desktop", "extension", "host", "tool"]},
            "process_ids": array(count()), "tools": array(tool),
            "processes": {"type": "array", "items": {"type": "object"}},
            "folders": {"type": "array", "items": {"type": "object"}},
            "ports": {"type": "array", "items": {"type": "object"}},
            "links": {"type": "array", "items": {"type": "object"}},
            "access": {"type": "object"}, "memory_bytes": count(), "cpu_percent": {"type": "number"},
            "more_processes": count(), "more_tools": count(), "more_folders": count(), "more_links": count()
        }),
        &[
            "id",
            "product",
            "name",
            "vendor",
            "pid",
            "kind",
            "process_ids",
            "tools",
            "processes",
            "folders",
            "ports",
            "links",
            "access",
            "memory_bytes",
            "cpu_percent",
            "more_processes",
            "more_tools",
            "more_folders",
            "more_links",
        ],
    );
    object(
        json!({
            "agents": array(agent), "platform": {"type": "string"},
            "taken_at_ms": count(), "limits": strings()
        }),
        &["agents", "platform", "taken_at_ms", "limits"],
    )
}

fn output_schema(name: &str) -> Value {
    match name {
        "list_ports" => {
            let row = object(
                json!({
                    "port": port(), "protocol": {"type": "string", "enum": ["tcp", "udp"]},
                    "addresses": {"type": "array"}, "pid": {"type": ["integer", "null"]},
                    "process": {"type": ["string", "null"]}, "command": {"type": ["string", "null"]},
                    "label": {"type": "string"}, "framework": {"type": ["string", "null"]},
                    "project": {"type": ["object", "null"]}, "container": {"type": ["object", "null"]},
                    "is_dev": {"type": "boolean"}, "exposure": {"type": "string"}, "protected": {"type": "boolean"}
                }),
                &[
                    "port",
                    "protocol",
                    "addresses",
                    "pid",
                    "process",
                    "command",
                    "label",
                    "framework",
                    "project",
                    "container",
                    "is_dev",
                    "exposure",
                    "protected",
                ],
            );
            object(
                json!({
                    "ports": array(row), "hidden_sockets": count(),
                    "warnings": strings(), "platform": {"type": "string"}, "taken_at_ms": count()
                }),
                &[
                    "ports",
                    "hidden_sockets",
                    "warnings",
                    "platform",
                    "taken_at_ms",
                ],
            )
        }
        "list_agents" => agents_schema(),
        "find_free_port" => object(json!({"port": port()}), &["port"]),
        "wait_for_port" => object(
            json!({"port": port(), "ready": {"type": "boolean"}, "elapsed_ms": count()}),
            &["port", "ready", "elapsed_ms"],
        ),
        "get_topology" => object(
            json!({
                "nodes": {"type": "array", "items": {"type": "object"}},
                "edges": {"type": "array", "items": {"type": "object"}},
                "clusters": {"type": "array", "items": {"type": "object"}},
                "stats": {"type": "object"}, "taken_at_ms": count()
            }),
            &["nodes", "edges", "clusters", "stats", "taken_at_ms"],
        ),
        "plan_cluster_stop" => plan_schema(),
        "explain_port" => object(
            json!({
                "port": port(), "status": {"type": "string", "enum": ["free", "busy", "reserved"]},
                "headline": {"type": "string"}, "recommendation": {"type": "string"}, "commands": strings(),
                "owners": {"type": "array", "items": {"type": "object"}},
                "entries": {"type": "array", "items": {"type": "object"}},
                "details": {"type": "array", "items": {"type": "object"}},
                "plan": {"type": ["object", "null"]}, "http": {"type": ["object", "null"]}
            }),
            &[
                "port",
                "status",
                "headline",
                "recommendation",
                "commands",
                "owners",
                "entries",
                "details",
                "plan",
                "http",
            ],
        ),
        "stop_agent_ports" => object(
            json!({"agents": strings(), "plans": array(plan_schema()), "reports": array(stop_report_schema())}),
            &["plans", "reports"],
        ),
        "stop_port" => {
            // Dry-run ActionPlan or executed StopReport, both unchanged.
            json!({"type": "object", "anyOf": [plan_schema(), stop_report_schema()]})
        }
        _ => unreachable!("every published tool has an output contract"),
    }
}
