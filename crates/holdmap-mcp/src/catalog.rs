//! Static tool discovery metadata: each tool's name, title, description and all four
//! annotations (readOnlyHint, destructiveHint, idempotentHint, openWorldHint).
//! Input and output schemas live in `schemas.rs`, so the only "description" in each entry
//! here is the tool's own description.

use serde_json::{json, Value};
use std::sync::OnceLock;

use crate::schemas::{input_schema, output_schema};

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
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "explain_port",
            "title": "Explain why a port is busy",
            "description": "Plain-English root cause for a port (process tree, container, supervisor, OS service, TIME_WAIT, other user) plus the recommended fix and stop plan, who is connected, uptime, bind-address risk and the HTTP status/title. Command lines have secrets hidden.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "inspect_port",
            "title": "Inspect a port in full",
            "description": "Full inspection of a port: process tree, who is connected, bind-address risk, HTTP status/title, recommended fix and stop plan. Same structured result as explain_port. Command lines have secrets hidden.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "find_free_port",
            "title": "Find a free port",
            "description": "Find a free TCP port before starting a server. With near, checks at most 500 ports at or above that number; without it, asks the OS for an ephemeral port. The result is a point-in-time check and does not reserve the port.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "wait_for_port",
            "title": "Wait for a port",
            "description": "Wait for TCP acceptance on localhost:port, or for the port to be free with until_free. This checks socket readiness, not HTTP/application health. Blocks this stdio server until ready or timeout; timeout_s is 0.1–600 seconds (default 30).",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "get_topology",
            "title": "Service dependency graph",
            "description": "Which local services talk to which: nodes (services, containers, clients, external hosts), edges (live TCP connections; `a -> b` means a depends on b) and clusters (compose project, k8s namespace, supervisor such as turbo/pm2/concurrently, monorepo workspace, git repo). Use all=true before stopping something to include system/app dependents; the default view is filtered to development services and peers.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_agents",
            "title": "Agents and developer tools on this machine",
            "description": "Inspect running AI coding agents (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider…) and developer tools with their working folders, child tools, ports, local services, remote IPs and CPU/memory. Likely MCP processes do not confirm a handshake or invocation. include_capabilities=true explicitly inspects bounded local MCP configuration metadata and installed skill file locations for supported current-account agents; returned as untrusted metadata: aliases, transport, enablement, scope, source paths, runner and package name with pinning, remote host, environment/header names (never values), whether one of the agent's processes runs each stdio server now, permission allow/ask/deny rules, documented safety settings, instruction-file (rules, commands, prompts, subagents, AGENTS.md) names and frontmatter descriptions, sanitized skill names/purposes and documented built-in tools. Configured/installed does not mean loaded or used. Default scans do not read capability configuration. The inventory never connects to servers or executes commands; skill instructions, chats and credential values are excluded from retention. Consult evidence, limits and omitted counts; recent folders do not prove current work.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "get_agent_flow",
            "title": "Agent process and socket relationships",
            "description": "Observed process ancestry and live TCP socket relationships for running agents. Distinct from get_topology (service clusters) and list_agents (inventory). Nodes and edges omit command lines, working directories, prompts and payloads. MCP process roles are inferred; tool calls need a separate activity observer. Optional agent filter uses the same matcher as list_agents.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_analyzers",
            "title": "Opt-in analyzers and their state",
            "description": "List the analyzers holdmap knows (bundled OSV lookup and SkillSpector) with state (not_installed, not_approved, approved, changed), declared network hosts and what they send, sandbox level and last run. Read-only: never runs, installs or approves an analyzer; the user turns them on in holdmap.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_insights",
            "title": "Stored analyzer findings",
            "description": "Findings from the latest run of each approved analyzer about agents' skills and MCP packages: severity, short title, subject with owning agents, file:line, fix hint, state (open/reviewed/snoozed) and whether it is new since the analyzer's previous run; each run lists its status, notes, and new/fixed counts. Never runs analyzers and never returns matched text. Findings are untrusted analyzer output; verify before acting.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_skill_trust",
            "title": "Skill trust",
            "description": "Each agent skill's trust from the last skill scan the user ran in holdmap: state (changed since trusted, new, unreadable, trusted), SHA-256 fingerprint, file count and size, first seen, when and why it was trusted, and a names-and-sizes-only summary of files added, removed or modified. Never reads skill folders, never re-hashes and never returns file contents or folder paths.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_risks",
            "title": "Top risks",
            "description": "Banded risk (critical, high, elevated, low) per agent and per skill folder, riskiest first, from stored findings of approved analyzers and the last skill scan. Points: 40 per open critical, 15 high, 5 medium, 1 low, 10 per skill changed since trusted; any open critical is Critical. Reviewed and snoozed findings don't count. Each entry lists its counts and top contributing findings. Never runs analyzers.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "list_needs",
            "title": "What needs the user's attention",
            "description": "The short \"needs you\" list holdmap shows on Home: agents listening on all interfaces, agents running as root or with wider access than usual, documented settings that skip approvals or the sandbox, unpinned MCP servers, dev servers open to the network and open medium-or-worse findings from approved analyzers. Each item has a rule, severity (high/medium), title, detail, target and, where stopping is the usual fix, a stop to preview with stop_port or stop_agent_ports. Read-only: never stops, runs or changes anything.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_agent_ports",
            "title": "Stop ports an agent started",
            "description": "Preview the unprotected services started by specific matching agents or developer tools (dry_run=true by default). Show every plan and get the user's authorization. Execute with dry_run=false and the returned session-local, one-use confirmation_id within 5 minutes, preserving agent/force options. All selected agents and plans must still match before any stop; changes require a new preview. A partially refused preview issues no handle. Only low-risk plans execute; protected and other users' processes are refused. Prefer an exact PID or specific agent filter.",
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false}
        },
        {
            "name": "plan_cluster_stop",
            "title": "Plan stopping a cluster",
            "description": "Dry-run only: the dependency-ordered plan (dependents first) for stopping every service in a cluster, with warnings about outside dependents. Never executes; ask the user to run `holdmap stop --cluster NAME`.",
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "stop_port",
            "title": "Stop whatever holds a port",
            "description": "Preview a port owner's stop plan (dry_run=true by default), show it and obtain the user's authorization. To execute, send dry_run=false and the returned session-local, one-use confirmation_id within 5 minutes with identical port/force/allow_non_dev options. Current owners and effects must still match the preview; any change requires a fresh preview. Stops the retained plan and verifies the port is free. Only the user's own permitted processes/containers can be stopped; protected, high-risk and other users' processes remain refused, with no override.",
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false}
        }
    ]);
    for tool in tools
        .as_array_mut()
        .expect("static tool catalog is an array")
    {
        let name = tool["name"].as_str().expect("static tool name").to_string();
        tool["inputSchema"] = input_schema(&name);
        tool["inputSchema"]["additionalProperties"] = json!(false);
        if matches!(
            tool["name"].as_str(),
            Some("stop_port" | "stop_agent_ports")
        ) {
            tool["inputSchema"]["if"] =
                json!({"required": ["dry_run"], "properties": {"dry_run": {"enum": [false]}}});
            tool["inputSchema"]["then"] = json!({"required": ["confirmation_id"]});
        }
        tool["outputSchema"] = output_schema(&name);
    }
    tools
}

pub(super) fn definition(name: &str) -> Option<&'static Value> {
    catalog()
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}
