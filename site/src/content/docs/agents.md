---
title: Agents, tools and apps
description: See which AI agents and developer tools are running, what they can reach, and stop only the ports they started.
order: 2
---

portwise finds the **AI coding agents** and **developer tools** on your machine and shows, for
each one, the folders it works in, the ports and apps it started, the services and hosts it talks
to, and what the OS says about its access. Every fact is marked seen, inferred or unknown. Chats,
settings and tokens are never read.

Recognized products include Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider,
Cline, Goose, Docker Desktop, OrbStack, Podman Desktop and others.

## From the CLI

```sh
portwise agents                         # every agent and tool, with its footprint
portwise agents claude                  # filter by product name or PID
portwise agents claude --stop-ports --dry-run   # plan only
portwise agents claude --stop-ports --yes       # stop unprotected ports that agent started
portwise agents --json                  # machine-readable report
```

Each entry shows memory, CPU, process count, how many ports are stoppable, the parent process,
child processes grouped as tools & apps it started, folder source labels (working dir / child cwd /
recent), ports with roles, connections, and access facts.

## Desktop and TUI

- **Desktop:** `⇧A` opens the Agents map. Expand a card to reveal a folder, open it in your editor,
  or stop the unprotected ports that agent started. Paths are verified against the current report
  before anything opens.
- **TUI:** Tab cycles Ports → Graph → Agents. Select an agent and press `x` (or `X`) to stop its
  unprotected ports.

## Access and evidence

Access covers the account the process runs as, sandbox and approval flags, network listeners, and
(on macOS) protected folders. Each fact carries evidence: **seen**, **inferred** or **unknown**.
Unknown is labelled on cards so you never mistake a guess for a measurement.

## Safety

Stopping from the Agents surface only targets unprotected ports the agent started (dev servers and
services). The agent’s own IDE and auth listeners (`PortRole::Agent`) are skipped. Protected
processes are never stopped unless you pass `--allow-protected` on the CLI; MCP always refuses them.

## Related

- [Desktop app](desktop/): shortcuts and the Agents map.
- [MCP for AI agents](mcp/): `list_agents` and `stop_agent_ports`.
- [CLI reference](cli/): every flag for `portwise agents`.
