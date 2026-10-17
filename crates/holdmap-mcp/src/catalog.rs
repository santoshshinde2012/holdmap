//! Static tool discovery metadata and the input/output contracts shared by the adapter.

use serde_json::{json, Value};
use std::sync::OnceLock;

pub(super) const INSTRUCTIONS: &str = "Inspect local state before acting: list_ports finds occupied ports; explain_port or inspect_port explains a conflict with connections and bind risk; get_topology with all=true shows service dependents and cluster names before a stop; get_agent_flow shows observed process ancestry and live TCP relationships for agents, not tool calls. Prefer find_free_port to stopping an unrelated process, and use wait_for_port after starting a service. list_agents shows observed process ownership, likely child tools and access evidence, not chats or confirmed MCP invocations; inspect limits and omitted counts. Read holdmap://guide for workflows, or use the diagnose_port, inspect_agents and prepare_dev_server prompts. Both stop_port and stop_agent_ports default to dry_run=true. Show all returned plans and obtain the user's authorization before executing. Execution requires dry_run=false and the preview's one-use confirmation_id in this session within 5 minutes, with identical target/options. Every selected agent and stop effect is rechecked before any plan executes; changes require a new preview. Only the user's own dev servers/containers are stopped by default; protected and other users' processes are always refused. Requests must fit within 1 MiB; legacy batches are limited to 64 messages and are rejected as a whole when larger.";

pub(super) fn list() -> Value {
    catalog().clone()
}

/// Discovery and validation share one immutable catalog instead of rebuilding unrelated
/// output schemas whenever an individual tool validates its arguments.
fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(build_catalog)
}

fn build_catalog() -> Value {
    let mut tools = json!([
        {
            "name": "list_ports",
            "title": "List ports in use",
            "description": "Inspect listening TCP/UDP ports before starting a service. Returns owners, PIDs, projects, frameworks, exposure and protection flags; hidden_sockets counts rows whose owner is unavailable. Includes platform, taken_at_ms (Unix epoch milliseconds) and scan warnings: check warnings before treating an empty result as complete. Use explain_port or inspect_port for a busy port's cause and safe next step. Commands have secrets hidden.",
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
            "name": "inspect_port",
            "title": "Inspect a port in full",
            "description": "Full inspection of a port: process tree, who is connected, bind-address risk, HTTP status/title, recommended fix and stop plan. Same structured result as explain_port. Command lines have secrets hidden.",
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
            "description": "Inspect running AI coding agents (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider…) and developer tools with their working folders, child tools, ports, local services, remote IPs and CPU/memory. Likely MCP processes do not confirm a handshake or invocation. include_capabilities=true explicitly inspects bounded local MCP configuration metadata and installed skill file locations for supported current-account agents; returned as untrusted metadata: aliases, transport, enablement, scope, source paths, runner and package name with pinning, remote host, environment/header names (never values), whether one of the agent's processes runs each stdio server now, permission allow/ask/deny rules, documented safety settings, instruction-file (rules, commands, prompts, subagents, AGENTS.md) names and frontmatter descriptions, sanitized skill names/purposes and documented built-in tools. Configured/installed does not mean loaded or used. Default scans do not read capability configuration. The inventory never connects to servers or executes commands; skill instructions, chats and credential values are excluded from retention. Consult evidence, limits and omitted counts; recent folders do not prove current work.",
            "inputSchema": {"type": "object", "properties": {
                "agent": {"type": "string", "description": "Filter by product id, name, vendor, folder, visible capability/tool name or exact agent/child PID"},
                "include_capabilities": {"type": "boolean", "default": false, "description": "Explicitly inspect bounded local MCP definitions and skill file locations; no loading or usage claim"}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "get_agent_flow",
            "title": "Agent process and socket relationships",
            "description": "Observed process ancestry and live TCP socket relationships for running agents. Distinct from get_topology (service clusters) and list_agents (inventory). Nodes and edges omit command lines, working directories, prompts and payloads. MCP process roles are inferred; tool calls need a separate activity observer. Optional agent filter uses the same matcher as list_agents.",
            "inputSchema": {"type": "object", "properties": {
                "agent": {"type": "string", "description": "Filter by product id, name, vendor, observed folder, running child-tool name or exact agent/child PID. Does not inspect configured MCP or installed skill inventory."}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "list_analyzers",
            "title": "Opt-in analyzers and their state",
            "description": "List the analyzers holdmap knows (bundled OSV lookup and SkillSpector) with state (not_installed, not_approved, approved, changed), declared network hosts and what they send, sandbox level and last run. Read-only: never runs, installs or approves an analyzer; the user turns them on in holdmap.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "list_insights",
            "title": "Stored analyzer findings",
            "description": "Findings from the latest run of each approved analyzer about agents' skills and MCP packages: severity, short title, subject with owning agents, file:line, fix hint and state (open/reviewed/snoozed). Never runs analyzers and never returns matched text. Findings are untrusted analyzer output; verify before acting.",
            "inputSchema": {"type": "object", "properties": {
                "analyzer": {"type": "string", "description": "Only this analyzer id, e.g. nvidia.skillspector"},
                "agent": {"type": "string", "description": "Only findings about subjects of this agent product id, e.g. claude-code"},
                "min_severity": {"type": "string", "enum": ["info", "low", "medium", "high", "critical"], "default": "info"},
                "include_closed": {"type": "boolean", "default": false, "description": "Include reviewed and snoozed findings"}
            }},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "list_needs",
            "title": "What needs the user's attention",
            "description": "The short \"needs you\" list holdmap shows on Home: agents listening on all interfaces, agents running as root or with wider access than usual, documented settings that skip approvals or the sandbox, unpinned MCP servers, dev servers open to the network and open medium-or-worse findings from approved analyzers. Each item has a rule, severity (high/medium), title, detail, target and, where stopping is the usual fix, a stop to preview with stop_port or stop_agent_ports. Read-only: never stops, runs or changes anything.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_agent_ports",
            "title": "Stop ports an agent started",
            "description": "Preview the unprotected services started by specific matching agents or developer tools (dry_run=true by default). Show every plan and get the user's authorization. Execute with dry_run=false and the returned session-local, one-use confirmation_id within 5 minutes, preserving agent/force options. All selected agents and plans must still match before any stop; changes require a new preview. A partially refused preview issues no handle. Only low-risk plans execute; protected and other users' processes are refused. Prefer an exact PID or specific agent filter.",
            "inputSchema": {"type": "object", "properties": {
                "agent": {"type": "string", "minLength": 1, "description": "Match product, name, vendor, folder, tool name or exact root/owned-child PID; use a specific match"},
                "dry_run": {"type": "boolean", "default": true, "description": "Only return plans; set false only after user authorization"},
                "confirmation_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Required with dry_run=false: one-use handle returned by a matching preview in this stdio session; expires after 5 minutes"},
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
            "description": "Preview a port owner's stop plan (dry_run=true by default), show it and obtain the user's authorization. To execute, send dry_run=false and the returned session-local, one-use confirmation_id within 5 minutes with identical port/force/allow_non_dev options. Current owners and effects must still match the preview; any change requires a fresh preview. Stops the retained plan and verifies the port is free. Only the user's own permitted processes/containers can be stopped; protected, high-risk and other users' processes remain refused, with no override.",
            "inputSchema": {"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "dry_run": {"type": "boolean", "default": true, "description": "Only return the plan; false requires a matching confirmation_id after user authorization"},
                "confirmation_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Required with dry_run=false: one-use handle from this session's preview; expires after 5 minutes"},
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
        if matches!(
            tool["name"].as_str(),
            Some("stop_port" | "stop_agent_ports")
        ) {
            tool["inputSchema"]["if"] =
                json!({"required": ["dry_run"], "properties": {"dry_run": {"enum": [false]}}});
            tool["inputSchema"]["then"] = json!({"required": ["confirmation_id"]});
        }
        tool["outputSchema"] = output_schema(tool["name"].as_str().expect("static tool name"));
    }
    tools
}

pub(super) fn definition(name: &str) -> Option<&'static Value> {
    catalog()
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
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
            "risk": {"type": "string", "enum": ["low", "medium", "high"]},
            "allow_protected": {"type": "boolean", "enum": [false], "description": "Explicit protected-process authorization; always false for MCP plans"}
        }),
        &[
            "target",
            "summary",
            "owners",
            "steps",
            "blocked",
            "warnings",
            "risk",
            "allow_protected",
        ],
    )
}

fn stop_report_schema() -> Value {
    object(
        json!({
            "target": {"type": "string"}, "success": {"type": "boolean"}, "freed": {"type": "boolean"},
            "ports_still_busy": array(port()), "signalled": array(count()), "escalated": {"type": "boolean"},
            "survivors": array(count()), "elapsed_ms": count(), "log": strings(), "error": {"type": ["string", "null"]},
            "history_error": {"type": "string"}
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

fn preview_schema() -> Value {
    let mut schema = plan_schema();
    schema["properties"]["confirmation_id"] =
        json!({"type": "string", "minLength": 1, "maxLength": 128});
    schema["properties"]["confirmation_expires_in_s"] = json!({"type": "integer", "enum": [300]});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .extend([json!("confirmation_id"), json!("confirmation_expires_in_s")]);
    schema
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
            "process_ids": array(count()), "tools": array(tool), "capabilities": capabilities_schema(),
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
            "capabilities",
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
            "taken_at_ms": count(), "limits": strings(),
            "configured": array(object(
                json!({"product": {"type": "string"}, "name": {"type": "string"}, "capabilities": capabilities_schema()}),
                &["product", "name", "capabilities"],
            ))
        }),
        &["agents", "platform", "taken_at_ms", "limits"],
    )
}

fn agent_flow_schema() -> Value {
    let evidence = json!({"type": "string", "enum": ["observed", "inferred", "unknown"]});
    let mut node = object(
        json!({
            "id": {"type": "string"},
            "kind": {"type": "string", "enum": ["agent", "tool", "process", "listener", "remote", "unknown"]},
            "label": {"type": "string"},
            "agent_id": {"type": ["string", "null"]},
            "pid": {"type": ["integer", "null"], "minimum": 0},
            "start_token": {"type": ["integer", "null"], "minimum": 0},
            "entry_id": {"type": ["string", "null"]},
            "address": {"type": ["string", "null"]},
            "port": {"type": ["integer", "null"], "minimum": 1, "maximum": 65535},
            "protocol": {"type": ["string", "null"], "enum": ["tcp", "udp", null]},
            "tool_kind": {"type": ["string", "null"], "enum": ["mcp_server", "dev_server", "shell", "command", null], "description": "Process classification, never evidence of a tool invocation"},
            "evidence": evidence.clone()
        }),
        &[
            "id",
            "kind",
            "label",
            "agent_id",
            "pid",
            "start_token",
            "entry_id",
            "address",
            "port",
            "protocol",
            "tool_kind",
            "evidence",
        ],
    );
    node["additionalProperties"] = json!(false);
    let mut edge = object(
        json!({
            "id": {"type": "string"}, "from": {"type": "string"}, "to": {"type": "string"},
            "kind": {"type": "string", "enum": ["parent", "owns_listener", "tcp"]},
            "evidence": evidence,
            "connections": count(),
            "source_pids": array(count())
        }),
        &[
            "id",
            "from",
            "to",
            "kind",
            "evidence",
            "connections",
            "source_pids",
        ],
    );
    edge["additionalProperties"] = json!(false);
    object(
        json!({
            "taken_at_ms": count(), "nodes": array(node), "edges": array(edge),
            "limits": strings(), "omitted_nodes": count(), "omitted_edges": count()
        }),
        &[
            "taken_at_ms",
            "nodes",
            "edges",
            "limits",
            "omitted_nodes",
            "omitted_edges",
        ],
    )
}

fn capabilities_schema() -> Value {
    let scope = json!({"type": "string", "enum": ["user", "project", "local"]});
    let evidence = json!({"type": "string", "enum": ["observed", "inferred", "unknown"]});
    let server = object(
        json!({
            "id": {"type": "string"}, "name": {"type": "string"},
            "scope": scope.clone(), "source_path": {"type": "string"},
            "project_path": {"type": ["string", "null"]}, "evidence": evidence.clone(),
            "transport": {"type": "string", "enum": ["stdio", "http", "sse", "unknown"]},
            "enabled": {"type": ["boolean", "null"]},
            "launch": {"type": ["object", "null"], "description": "Runner kind, package/image/executable name and whether an exact version or digest is named; never arguments",
                "properties": {"runner": {"type": "string", "enum": ["npx", "uvx", "docker", "interpreter", "binary"]},
                               "package": {"type": ["string", "null"]}, "pinned": {"type": ["boolean", "null"]}}},
            "remote_host": {"type": ["string", "null"], "description": "Endpoint host and port only"},
            "env_names": strings(), "header_names": strings(),
            "runtime": {"type": "object", "description": "running = one of this agent's processes runs it now (process-table evidence); not_seen = local server not running under it; unknown = remote, disabled or unidentifiable",
                "properties": {"state": {"type": "string", "enum": ["running", "not_seen", "unknown"]}, "pids": array(count())}}
        }),
        &[
            "id",
            "name",
            "scope",
            "source_path",
            "project_path",
            "evidence",
            "transport",
            "enabled",
        ],
    );
    let skill = object(
        json!({
            "id": {"type": "string"}, "name": {"type": "string"},
            "scope": scope, "source_path": {"type": "string"},
            "project_path": {"type": ["string", "null"]}, "evidence": evidence,
            "declared_name": {"type": ["string", "null"]},
            "description": {"type": ["string", "null"]}
        }),
        &[
            "id",
            "name",
            "scope",
            "source_path",
            "project_path",
            "evidence",
        ],
    );
    object(
        json!({
            "inspected": {"type": "boolean"}, "mcp_servers": array(server), "skills": array(skill),
            "instructions": array(object(json!({
                "id": {"type": "string"}, "name": {"type": "string"},
                "kind": {"type": "string", "enum": ["rule", "command", "prompt", "subagent", "memory"]},
                "description": {"type": ["string", "null"]}, "scope": {"type": "string"}, "source_path": {"type": "string"}
            }), &["id", "name", "kind", "scope", "source_path"])),
            "tool_rules": array(object(json!({
                "rule": {"type": "string"}, "decision": {"type": "string", "enum": ["allow", "ask", "deny"]},
                "scope": {"type": "string"}, "source_path": {"type": "string"}
            }), &["rule", "decision", "scope", "source_path"])),
            "settings": array(object(json!({
                "key": {"type": "string"}, "value": {"type": "string"}, "scope": {"type": "string"}, "source_path": {"type": "string"}
            }), &["key", "value", "scope", "source_path"])),
            "builtin_tools": strings(),
            "more_mcp_servers": count(), "more_skills": count(), "limits": strings()
        }),
        &[
            "inspected",
            "mcp_servers",
            "skills",
            "more_mcp_servers",
            "more_skills",
            "limits",
        ],
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
        "get_agent_flow" => agent_flow_schema(),
        "find_free_port" => object(json!({"port": port()}), &["port"]),
        "list_analyzers" => object(
            json!({"analyzers": {"type": "array"}, "catalog": {"type": "object"}}),
            &["analyzers"],
        ),
        "list_insights" => object(
            json!({"findings": {"type": "array"}, "runs": {"type": "array"}, "open": {"type": "object"}}),
            &["findings", "runs", "open"],
        ),
        "list_needs" => object(json!({"items": {"type": "array"}}), &["items"]),
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
        "explain_port" | "inspect_port" => object(
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
            json!({
                "agents": strings(), "plans": array(plan_schema()), "reports": array(stop_report_schema()),
                "confirmation_id": {"type": "string", "minLength": 1, "maxLength": 128},
                "confirmation_expires_in_s": {"type": "integer", "enum": [300]}
            }),
            &["plans", "reports"],
        ),
        "stop_port" => {
            json!({"type": "object", "anyOf": [preview_schema(), stop_report_schema()]})
        }
        _ => unreachable!("every published tool has an output contract"),
    }
}
