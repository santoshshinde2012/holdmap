---
title: MCP for AI agents
description: Connect coding assistants to holdmap over MCP for ports, agent inventory, observed relationships and reviewed service stops.
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

The server negotiates MCP `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`. Older
negotiated versions keep their compatible discovery and result shapes; newer clients receive
structured tool results alongside readable text. Always follow the schemas your installed
server advertises.

## Tools

The server has sixteen tools:

| Tool | What it does |
|---|---|
| `list_ports` | Listening ports with process, PID, project and framework, plus scan warnings, platform and collection time. Filters like `vite`, `:3000`, `3000-3999`. |
| `explain_port` | Plain-English root cause, the recommended fix and the stop plan, who's connected and the bind risk. |
| `inspect_port` | The same structured inspection as `explain_port`: connections, process tree, bind risk and bounded local HTTP probing. |
| `find_free_port` | A free TCP port, optionally the first one at or after `near`. |
| `wait_for_port` | Waits until a port accepts connections, or until it's free. |
| `get_topology` | Which local services talk to which, grouped into clusters. |
| `list_agents` | Running AI coding agents, child tools and likely MCP servers, folders, ports, connections, CPU, memory and access facts. Filter by agent, owned PID, folder or tool name. Set `include_capabilities: true` to inspect supported MCP definitions and skill locations. Read-only. |
| `get_agent_flow` | Observed process ancestry and live TCP relationships for agents, with typed nodes, relationships, evidence and collection limits. It does not inspect capability files or report tool invocations. |
| `list_analyzers` | Opt-in analyzers (SkillSpector, OSV lookup and ones you added), their state, source, sandbox, declared network use, last run and the signed catalog version. Never runs, installs, adds or approves one. |
| `list_insights` | Stored findings from approved analyzers about agents' skills and MCP packages, filtered by analyzer, agent, severity or state. Never runs analyzers. |
| `list_skill_trust` | Each skill's trust from the last scan the user ran: changed since trusted, new or trusted, with fingerprint, first seen and a names-and-sizes-only change summary. Never reads folders. |
| `list_risks` | Banded risk per agent and per skill folder (named by its skills, never a path) from stored findings and skill trust, with the counts and findings behind each band. Never runs analyzers. |
| `list_needs` | The "needs you" list from Home: exposed agents, root or wider access, risky settings, unpinned MCP servers, open dev servers and analyzer findings, with a stop to preview where that's the usual fix. Read-only. |
| `stop_agent_ports` | Preview the unprotected ports started by matching agents; execute the retained plans with a one-use `confirmation_id`. Defaults to `dry_run: true`. |
| `plan_cluster_stop` | A dry-run, dependency-ordered plan for stopping a cluster. Never executes. |
| `stop_port` | Preview a port owner's stop plan; execute with a matching one-use `confirmation_id` and verify the port is free. Defaults to `dry_run: true`. |

Tools publish input and output schemas. Results include a structured JSON object and a readable
summary with the same JSON for older clients. Invalid argument types, ranges, enum values or
unknown options return an error before the tool acts. For example, `dry_run` must be the JSON
boolean `true`, rather than the string `"true"`.
A `tools/call` request must omit `arguments` or provide an object; arrays, explicit `null`
and scalars are malformed request envelopes (`-32602`). A well-formed object that fails a
tool's input schema or encounters a domain error instead returns a tool error (`isError`).

Explicitly request configuration metadata using
`{"name":"list_agents","arguments":{"include_capabilities":true}}` inside a `tools/call`
request. The default is false. Supported bounded configuration files can contain credentials;
only metadata is returned: aliases, transport, enablement, scope and source paths; for each MCP
server its runner and package (pinned or not), remote host, environment/header **names** (never
values) and whether one of the agent's processes runs it now;
plus permission rules, documented safety settings, instruction-file names and
documented built-in tools. Holdmap never starts or connects to an MCP server. Skill identity
comes from folders, with optional sanitized declared name/purpose from bounded YAML headers;
instruction bodies are not retained. Installed definitions do not prove loading, connection
or use and do not grant process-action authority. See [MCP & skills coverage](../agents/#mcp-and-skills).

Inspect `list_ports.warnings` before treating an empty result as a complete scan.
`taken_at_ms` is the collection time in Unix epoch milliseconds; `platform` identifies the
system scanned. Warnings also appear in the readable summary.

## Tool annotations

Every tool declares all four MCP hints as explicit booleans, so clients can warn before a tool
acts:

| Tools | `readOnlyHint` | `destructiveHint` | `idempotentHint` | `openWorldHint` |
|---|---|---|---|---|
| The 14 read and plan tools (every tool except the two below) | `true` | `false` | `true` | `false` |
| `stop_port`, `stop_agent_ports` | `false` | `true` | `false` | `false` |

The hints sit next to each tool's name and description in
[`crates/holdmap-mcp/src/catalog.rs`](https://github.com/santoshshinde2012/holdmap/blob/main/crates/holdmap-mcp/src/catalog.rs).
Input and output schemas live in
[`crates/holdmap-mcp/src/schemas.rs`](https://github.com/santoshshinde2012/holdmap/blob/main/crates/holdmap-mcp/src/schemas.rs),
so a field's description is never read as a tool's description. A test checks that each of
the 16 entries has one description and all four hints.

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
The `inspect_agents` template also uses `get_agent_flow` for observed socket relationships.

For clients exposing tools alone, ask for tasks such as:

- “Explain what owns port 3000 and show its dependencies.”
- “Find a free port near 3000, then check whether my server is ready after I start it.”
- “Show the agents working in shop-web, including child tools and collection limits.”
- “Show observed process and socket relationships for the Codex agent.”

A free port is a candidate, not a reservation. TCP readiness does not establish application
health. Likely MCP servers are inferred from process metadata. Passive inspection does not
record individual tool calls or verify their transport. Optional
[instrumented Agent Flow recording](../agents/#set-up-optional-recording) receives metadata from integrations you configure.
Recent folders are current-account history, attached only when the agent's account matches;
they do not establish current work by a particular agent instance.

Before considering a stop, request `get_topology` with `all: true` to include system and
application services. Its default view is filtered to development services and their peers.

## Preview and execute a stop

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

## Record this server's tool metadata

Ordinary `holdmap mcp` does not record calls. To opt in, use **Start recording** in the desktop's
Calls view, or keep `holdmap activity serve --duration 10m` running for a separate CLI session.
Take the endpoint and private token from the desktop Setup example or the CLI's startup output, then pass
`HOLDMAP_ACTIVITY_ENDPOINT` and `HOLDMAP_ACTIVITY_TOKEN` through the MCP client's environment.
Add `--record-activity` to the server's arguments:

```json
{"command":"holdmap","args":["mcp","--record-activity"]}
```

This observes only known tools called on this Holdmap MCP server. It reports the tool name,
status, timing and serialized request/result byte counts; arguments, results, raw request IDs
and error text are not given to the recorder. Byte counts are not network traffic measurements.
The desktop shows records sent to its own collector; a CLI collector is read with
`holdmap activity snapshot --json` using that collector's environment variables.

Keep the token private and update the client's environment when a recording restarts. Metadata
delivery failures leave the MCP tools available. See [Agent Flow setup and privacy](../agents/#set-up-optional-recording)
for retention, instrumentation and visibility limits.

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
