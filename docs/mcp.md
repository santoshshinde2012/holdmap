# portwise MCP server

`portwise mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server that runs over
stdio (newline-delimited JSON-RPC 2.0). It lets AI coding agents see which ports are in use, ask why
one is busy, find a free port, wait for a server to come up, read the service graph and, within strict
limits, stop their own dev servers.

It uses the same engine and the same safety rules as the CLI and the desktop app, and it is the
strictest of the four surfaces: it never overrides protection.

## Contents

- [Set up a client](#set-up-a-client)
- [Tools](#tools)
- [Safety rules for agents](#safety-rules-for-agents)
- [Example session](#example-session)
- [Troubleshooting](#troubleshooting)

## Set up a client

Install the `portwise` binary first (see the [README](../README.md#install)). The server needs no
arguments, network access or API keys.

### Claude Desktop

Edit `claude_desktop_config.json` (Settings → Developer → Edit Config). It lives in
`~/Library/Application Support/Claude/` on macOS and `%APPDATA%\Claude\` on Windows.

```json
{
  "mcpServers": {
    "portwise": { "command": "portwise", "args": ["mcp"] }
  }
}
```

Restart Claude Desktop afterwards.

### Claude Code

```sh
claude mcp add portwise -- portwise mcp
```

### Cursor

Add the server to `~/.cursor/mcp.json` (every project) or `.cursor/mcp.json` (one project):

```json
{
  "mcpServers": {
    "portwise": { "command": "portwise", "args": ["mcp"] }
  }
}
```

### VS Code

VS Code uses a `servers` key and an explicit `type`. Put this in `.vscode/mcp.json` in a workspace,
or run **MCP: Open User Configuration** for every workspace:

```json
{
  "servers": {
    "portwise": { "type": "stdio", "command": "portwise", "args": ["mcp"] }
  }
}
```

### Other clients

Any client that can launch a stdio server works: the command is `portwise` and the only argument
is `mcp`. Global options such as `--no-docker` go before or after it (`portwise mcp --no-docker`).

## Tools

The server supports protocol versions `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`,
answers with the client's version when it is one of those, and otherwise offers the newest. It
also answers `ping` and `tools/list`. Every result has a short text
summary followed by the JSON, and the same JSON in `structuredContent`.

| Tool | Arguments | Read-only | What it returns |
|---|---|---|---|
| `list_ports` | `query` (string, the CLI query language), `dev_only` (bool), `protocol` (`tcp` or `udp`) | yes | Listening ports with process, PID, project and framework |
| `explain_port` | `port` (1–65535, required) | yes | The root cause, the recommended fix and the stop plan |
| `find_free_port` | `near` (port) | yes | A free TCP port, the first one at or after `near` if given |
| `wait_for_port` | `port` (required), `timeout_s` (default 30), `until_free` (default false) | yes | Whether the port started accepting connections (or became free) in time |
| `get_topology` | `all` (default false), `format` (`json`, `mermaid`, `dot` or `tree`; default `json`) | yes | Services, connections (`a -> b` means a depends on b) and clusters |
| `plan_cluster_stop` | `cluster` (id or name, required) | yes | The dependency-ordered plan for stopping a cluster. It never executes |
| `stop_port` | `port` (required), `dry_run`, `force`, `allow_non_dev` (all default false) | no | The plan, or the stop report after executing it |

The server also sends these instructions at initialisation: *use `explain_port` before stopping
anything, prefer `find_free_port` over stopping processes you didn't start, and remember that
`stop_port` only stops the user's own dev servers and containers by default.*

## Safety rules for agents

`stop_port` runs the same plan the CLI shows, with extra limits:

- **Low risk only, by default.** Your own dev servers and containers can be stopped. A medium-risk
  owner (one of your processes that isn't a dev server) needs `allow_non_dev: true`, which an agent
  should only pass when you asked for it.
- **Never high risk.** Protected processes (editors, terminals, agent hosts, system services,
  portwise itself) and other users' processes are always refused. There is no override.
- **Blocked plans stay blocked.** Ports that need elevation, OS features such as AirPlay Receiver,
  and supervisor-managed services return the explanation and the command for you to run.
- **Cluster stops are dry runs.** `plan_cluster_stop` returns the plan and asks you to run
  `portwise stop --cluster NAME` yourself.
- **`dry_run: true`** returns the plan without touching anything.

Refusals come back as tool errors (`isError: true`) with the reason and the command you could run.

## Example session

Requests are one JSON object per line on stdin; responses come back the same way on stdout.

```text
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"demo","version":"0"}}}
← {"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"instructions":"Use explain_port before stopping anything. …","protocolVersion":"2025-06-18","serverInfo":{"name":"portwise","title":"portwise","version":"0.1.0"}}}
→ {"jsonrpc":"2.0","method":"notifications/initialized"}
→ {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":3000,"dry_run":true}}}
← {"id":2,"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"Plan (dry run): Gracefully stop the npm run dev process tree (3 processes): SIGTERM, then SIGKILL after 5s, then verify port 3000 is free.\n…"}],…}}
→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_free_port","arguments":{"near":3000}}}
← {"id":3,"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"Port 3001 is free.\n\n{\n  \"port\": 3001\n}"}],"isError":false,"structuredContent":{"port":3001}}}
```

You can try this yourself by piping those lines into `portwise mcp`.

## Troubleshooting

- **"command not found" or the server never starts.** Desktop apps often don't inherit your shell's
  `PATH`. Use the absolute path in `command`: `~/.cargo/bin/portwise` (written out in full, e.g.
  `/Users/you/.cargo/bin/portwise`) or `C:\Users\you\.cargo\bin\portwise.exe`.
- **Ports owned by root or other users are missing.** The server runs as you, so it can't see
  other users' sockets. `explain_port` says how many are hidden and that elevation is needed.
- **Containers show up as `docker-proxy` or not at all.** The server can't reach the container
  runtime's socket. Check that `docker ps` works without sudo, or pass `--no-docker` to skip it.
- **Logs.** The server writes nothing but protocol messages to stdout. Errors in a request come
  back as JSON-RPC errors (`-32700` parse error, `-32601` unknown method, `-32602` unknown tool).
  A tool that runs but fails (a bad port, a refused stop) returns a result with `isError: true`.
