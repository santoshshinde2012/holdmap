---
title: MCP for AI agents
description: Connect coding agents to Holdmap tools, discover workflows, inspect agent activity and diagnose development services.
order: 4
---

`holdmap mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio, so
coding assistants can see what's on your ports and clean up after themselves without guessing
with `lsof` and `kill -9`.

## Set it up

**Claude Desktop** (`claude_desktop_config.json`) and **Cursor** (`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "holdmap": { "command": "holdmap", "args": ["mcp"] }
  }
}
```

**VS Code** (`.vscode/mcp.json`):

```json
{
  "servers": {
    "holdmap": { "type": "stdio", "command": "holdmap", "args": ["mcp"] }
  }
}
```

If the client can't find `holdmap`, use the full path, for example `/Users/you/.local/bin/holdmap`
(`command -v holdmap` prints it).

After connecting, the server advertises its tools, a guide resource and three workflow prompts.
Prompt and resource menus depend on the client; clients exposing tools alone can use the same
workflows below. Holdmap observes the machine where the server runs.

This page describes the **current source / Unreleased** MCP interface. The bundled guide,
prompts, schemas and mandatory stop confirmations are changes following v0.3.0. Downloadable
v0.3.0 servers expose the nine tools below with the previous stop protocol; they do not return
`confirmation_id`. Build the current source to use the safer preview-to-execution flow below,
or follow your installed version's discovered schemas until the next release.

## Tools

| Tool | What it does |
|---|---|
| `list_ports` | Listening ports with process, PID, project and framework, plus scan warnings, platform and collection time. Filters like `vite`, `:3000`, `3000-3999`. |
| `explain_port` | Plain-English root cause, the recommended fix and the stop plan, who's connected and the bind risk. |
| `find_free_port` | A free TCP port, optionally the first one at or after `near`. |
| `wait_for_port` | Waits until a port accepts connections, or until it's free. |
| `get_topology` | Which local services talk to which, grouped into clusters. |
| `list_agents` | Running AI coding agents, child tools and likely MCP servers, folders, ports, connections, CPU, memory and access facts. Filter by agent, owned PID, folder or tool name. Read-only. |
| `stop_agent_ports` | Preview the unprotected ports started by matching agents; execute the retained plans with a one-use `confirmation_id`. Defaults to `dry_run: true`. |
| `plan_cluster_stop` | A dry-run, dependency-ordered plan for stopping a cluster. Never executes. |
| `stop_port` | Preview a port owner's stop plan; execute with a matching one-use `confirmation_id` and verify the port is free. Defaults to `dry_run: true`. |

Tools publish input and output schemas. Results include a structured JSON object and a readable
summary with the same JSON for older clients. Invalid argument types, ranges, enum values or
unknown options return an error before the tool acts. For example, `dry_run` must be the JSON
boolean `true`, rather than the string `"true"`.

Inspect `list_ports.warnings` before treating an empty result as a complete scan.
`taken_at_ms` is the collection time in Unix epoch milliseconds; `platform` identifies the
system scanned. Warnings also appear in the readable summary.

## Discover workflows

The server exposes `holdmap://guide` through MCP resources. It explains which tool to choose,
how to interpret evidence and partial scans, and how to diagnose a conflict without disrupting
a service. It is bundled with the server, so reading it requires no scan or filesystem access.

Clients supporting MCP prompts can discover these templates:

| Prompt | Arguments | Workflow |
|---|---|---|
| `diagnose_port` | Required `port`, a string such as `"3000"` | Explain ownership and exposure, inspect dependencies, then offer another port or an authorized stop plan. |
| `prepare_dev_server` | Optional `near`, a string such as `"3000"` | Find a candidate port, use the project's normal launch command, then wait for TCP readiness. |
| `inspect_agents` | Optional `agent`, such as `"claude"`, `"shop-web"` or `"pid:1234"` | Inspect running agents, child tools, resource use, folders and access evidence. |

Selecting a prompt returns instructions; it does not run tools, launch servers or stop processes.
MCP prompt arguments are strings, while tool calls use the types in each tool's input schema.

For clients exposing tools alone, ask for tasks such as:

- “Explain what owns port 3000 and show its dependencies.”
- “Find a free port near 3000, then check whether my server is ready after I start it.”
- “Show the agents working in shop-web, including child tools and collection limits.”

A free port is a candidate, not a reservation. TCP readiness does not establish application
health. Likely MCP servers are inferred from process metadata; Holdmap does not trace tool
calls or verify their transport. Recent folders are current-account history, attached only when
the agent's account matches; they do not establish current work by a particular agent instance.

Before considering a stop, request `get_topology` with `all: true` to include system and
application services. Its default view is filtered to development services and their peers.

## Preview and execute a stop (Unreleased)

Both stop tools default to `dry_run: true`. A permitted preview returns a `confirmation_id`
and `confirmation_expires_in_s: 300`. Show all returned plans and obtain authorization for
those effects before executing. Keep the same stdio session and send the same target and options:

```json
{"name":"stop_port","arguments":{"port":3000,"dry_run":true}}
```

After reviewing the result, use the actual handle it returned:

```json
{"name":"stop_port","arguments":{"port":3000,"dry_run":false,"confirmation_id":"<returned handle>"}}
```

If the preview included `force` or `allow_non_dev`, preserve their values. For
`stop_agent_ports`, preserve `agent` and `force`, and review every per-port plan. A refused
bulk plan issues no handle. Before any stop, the server rechecks the entire selected agent set
and every plan's owners, services and effects. Changes require a new preview; the server
executes the retained plans after a successful check.

Handles are one-use, expire after five minutes and belong to one server session. They are
not authentication credentials or proof of user approval. Restarting or reinitializing the
server invalidates them. At most 32 previews are retained, so new previews can evict older ones.
Each preview is limited to 128 ports or selected agents and 1 MiB of retained data; use a
narrower filter for larger selections.

Malformed wire or schema inputs leave a handle available. A valid execution attempt consumes
it before checking options and collecting fresh state. Missing, expired, replayed, mismatched
or stale handles cause no stop. Collection or execution failures require a new preview.
Bulk execution stops on its first failure; it does not roll back a completed stop.

Stdio requests are limited to 1 MiB; legacy batches to 64 messages. Oversized frames and batches
are rejected before dispatching any of their requests.

## Guard rails

- `stop_port` only stops your own dev servers by default; anything else needs `allow_non_dev`.
- Process and supervisor stops require verified current-account ownership. Unknown and foreign
  accounts are refused even when `allow_non_dev` is enabled; unpinned systemd socket stops use
  the CLI instead. Agent bulk plans preserve each listener's TCP/UDP protocol.
- Protected processes (the OS, shells, terminals, IDEs, AI-assistant hosts, holdmap itself) are
  **always** refused over MCP, whatever the arguments.
- `stop_agent_ports` targets started services, skips protected listeners and defaults to a dry
  run. Inspect all per-port plans and use their matching `confirmation_id` with `dry_run: false`.
- Clients are told to show every plan and obtain your authorization. The server binds execution to
  the preview with `confirmation_id`; the client remains responsible for user approval.
- Recognized credential flags, headers and URL fields are redacted in command displays. Redaction
  is heuristic and cannot identify every arbitrary positional secret.
