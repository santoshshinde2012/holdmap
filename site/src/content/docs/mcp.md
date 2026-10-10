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

The bundled guide and prompts are development changes following v0.3.0. Released v0.3.0
servers expose the nine tools below; use those directly until the next release, or build the
development source to use the additional discovery capabilities.

## Tools

| Tool | What it does |
|---|---|
| `list_ports` | Listening ports with process, PID, project and framework, plus scan warnings, platform and collection time. Filters like `vite`, `:3000`, `3000-3999`. |
| `explain_port` | Plain-English root cause, the recommended fix and the stop plan, who's connected and the bind risk. |
| `find_free_port` | A free TCP port, optionally the first one at or after `near`. |
| `wait_for_port` | Waits until a port accepts connections, or until it's free. |
| `get_topology` | Which local services talk to which, grouped into clusters. |
| `list_agents` | Running AI coding agents, child tools and likely MCP servers, folders, ports, connections, CPU, memory and access facts. Filter by agent, owned PID, folder or tool name. Read-only. |
| `stop_agent_ports` | Plan or stop the unprotected ports started by matching agents; skips their IDE/auth listeners. Defaults to `dry_run: true`. |
| `plan_cluster_stop` | A dry-run, dependency-ordered plan for stopping a cluster. Never executes. |
| `stop_port` | Stops the owner of a port safely and verifies it's free. |

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

## Guard rails

- `stop_port` only stops your own dev servers by default; anything else needs `allow_non_dev`.
- Protected processes (the OS, shells, terminals, IDEs, AI-assistant hosts, holdmap itself) are
  **always** refused over MCP, whatever the arguments.
- `stop_agent_ports` targets started services, skips protected listeners and defaults to a dry
  run. Inspect its per-port plans before setting `dry_run: false`.
- Clients are told to show you the plan (`dry_run: true`) and get your OK before stopping anything.
- Passwords and tokens in command lines are redacted in every result.
