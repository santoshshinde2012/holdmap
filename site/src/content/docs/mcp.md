---
title: MCP for AI agents
description: Let Claude, Cursor or VS Code list ports, find a free one, wait for servers and stop their own dev servers safely.
order: 4
---

`portwise mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio, so
coding assistants can see what's on your ports and clean up after themselves without guessing
with `lsof` and `kill -9`.

## Set it up

**Claude Desktop** (`claude_desktop_config.json`) and **Cursor** (`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "portwise": { "command": "portwise", "args": ["mcp"] }
  }
}
```

**VS Code** (`.vscode/mcp.json`):

```json
{
  "servers": {
    "portwise": { "type": "stdio", "command": "portwise", "args": ["mcp"] }
  }
}
```

If the client can't find `portwise`, use the full path, for example `/Users/you/.local/bin/portwise`
(`command -v portwise` prints it).

## Tools

| Tool | What it does |
|---|---|
| `list_ports` | Listening ports with process, PID, project and framework. Filters like `vite`, `:3000`, `3000-3999`. |
| `explain_port` | Plain-English root cause, the recommended fix and the stop plan, who's connected and the bind risk. |
| `find_free_port` | A free TCP port, optionally the first one at or after `near`. |
| `wait_for_port` | Waits until a port accepts connections, or until it's free. |
| `get_topology` | Which local services talk to which, grouped into clusters. |
| `list_agents` | Agents and developer tools running here: folders, ports, connections and access (seen / inferred / unknown). Read-only. |
| `stop_agent_ports` | Stops the unprotected ports matching agents started. Prefer `dry_run: true` first. |
| `plan_cluster_stop` | A dry-run, dependency-ordered plan for stopping a cluster. Never executes. |
| `stop_port` | Stops the owner of a port safely and verifies it's free. |

## Guard rails

- `stop_port` only stops your own dev servers by default; anything else needs `allow_non_dev`.
- Protected processes (the OS, shells, terminals, IDEs, AI-assistant hosts, portwise itself) are
  **always** refused over MCP, whatever the arguments.
- Clients are told to show you the plan (`dry_run: true`) and get your OK before stopping anything.
- Passwords and tokens in command lines are redacted in every result.
