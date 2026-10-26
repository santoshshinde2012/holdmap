//! Input and output schemas of the MCP tools. Kept apart from `catalog.rs` so a field's
//! description is never read as a tool description.

use serde_json::{json, Value};

pub(crate) fn input_schema(name: &str) -> Value {
    match name {
        "list_ports" => json!({"type": "object", "properties": {
            "query": {"type": "string", "description": "Filter, e.g. 'vite', ':3000', '3000-3999', 'proto:udp'"},
            "dev_only": {"type": "boolean", "description": "Only likely development servers"},
            "protocol": {"type": "string", "enum": ["tcp", "udp"]}
        }}),
        "explain_port" => {
            json!({"type": "object", "properties": {"port": {"type": "integer", "minimum": 1, "maximum": 65535}}, "required": ["port"]})
        }
        "inspect_port" => {
            json!({"type": "object", "properties": {"port": {"type": "integer", "minimum": 1, "maximum": 65535}}, "required": ["port"]})
        }
        "find_free_port" => {
            json!({"type": "object", "properties": {"near": {"type": "integer", "minimum": 1, "maximum": 65535}}})
        }
        "wait_for_port" => json!({"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "timeout_s": {"type": "number", "minimum": 0.1, "maximum": 600, "default": 30},
                "until_free": {"type": "boolean", "default": false}
            }, "required": ["port"]}),
        "get_topology" => json!({"type": "object", "properties": {
            "all": {"type": "boolean", "default": false, "description": "Include system/app services, not only dev services and their peers"},
            "format": {"type": "string", "enum": ["json", "mermaid", "dot", "tree"], "default": "json"}
        }}),
        "list_agents" => json!({"type": "object", "properties": {
            "agent": {"type": "string", "description": "Filter by product id, name, vendor, folder, visible capability/tool name or exact agent/child PID"},
            "include_capabilities": {"type": "boolean", "default": false, "description": "Explicitly inspect bounded local MCP definitions and skill file locations; no loading or usage claim"}
        }}),
        "get_agent_flow" => json!({"type": "object", "properties": {
            "agent": {"type": "string", "description": "Filter by product id, name, vendor, observed folder, running child-tool name or exact agent/child PID. Does not inspect configured MCP or installed skill inventory."}
        }}),
        "list_analyzers" => json!({"type": "object", "properties": {}}),
        "list_insights" => json!({"type": "object", "properties": {
            "analyzer": {"type": "string", "description": "Only this analyzer id, e.g. nvidia.skillspector"},
            "agent": {"type": "string", "description": "Only findings about subjects of this agent product id, e.g. claude-code"},
            "min_severity": {"type": "string", "enum": ["info", "low", "medium", "high", "critical"], "default": "info"},
            "include_closed": {"type": "boolean", "default": false, "description": "Include reviewed and snoozed findings"},
            "new_only": {"type": "boolean", "default": false, "description": "Only findings new since each analyzer's previous run"}
        }}),
        "list_skill_trust" => json!({"type": "object", "properties": {
            "alerts_only": {"type": "boolean", "default": false, "description": "Only skills that changed since trusted, are new or are unreadable"}
        }}),
        "list_risks" => json!({"type": "object", "properties": {
            "by": {"type": "string", "enum": ["agent", "folder", "all"], "default": "all"},
            "min_band": {"type": "string", "enum": ["low", "elevated", "high", "critical"], "default": "low"}
        }}),
        "list_needs" => json!({"type": "object", "properties": {}}),
        "stop_agent_ports" => json!({"type": "object", "properties": {
                "agent": {"type": "string", "minLength": 1, "description": "Match product, name, vendor, folder, tool name or exact root/owned-child PID; use a specific match"},
                "dry_run": {"type": "boolean", "default": true, "description": "Only return plans; set false only after user authorization"},
                "confirmation_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Required with dry_run=false: one-use handle returned by a matching preview in this stdio session; expires after 5 minutes"},
                "force": {"type": "boolean", "default": false, "description": "SIGKILL immediately"}
            }, "required": ["agent"]}),
        "plan_cluster_stop" => {
            json!({"type": "object", "properties": {"cluster": {"type": "string", "minLength": 1, "description": "Cluster id or name from get_topology"}}, "required": ["cluster"]})
        }
        "stop_port" => json!({"type": "object", "properties": {
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "dry_run": {"type": "boolean", "default": true, "description": "Only return the plan; false requires a matching confirmation_id after user authorization"},
                "confirmation_id": {"type": "string", "minLength": 1, "maxLength": 128, "description": "Required with dry_run=false: one-use handle from this session's preview; expires after 5 minutes"},
                "force": {"type": "boolean", "default": false, "description": "SIGKILL immediately"},
                "allow_non_dev": {"type": "boolean", "default": false, "description": "Allow stopping a non-dev process of the user"}
            }, "required": ["port"]}),
        other => unreachable!("no input schema for {other}"),
    }
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

pub(crate) fn output_schema(name: &str) -> Value {
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
        "list_skill_trust" => object(
            json!({"skills": {"type": "array"}, "changed": count(), "untrusted": count(), "scanned_ms": {"type": ["integer", "null"]}}),
            &["skills", "changed", "untrusted"],
        ),
        "list_risks" => object(json!({"risks": {"type": "array"}}), &["risks"]),
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
