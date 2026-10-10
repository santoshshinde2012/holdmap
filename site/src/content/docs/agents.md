---
title: Agents, tools and apps
description: See which AI agents and developer tools are running, what they can reach, and stop only the ports they started.
order: 2
---

holdmap finds the **AI coding agents** and **developer tools** on your machine and shows, for
each one, the folders it works in, the ports and apps it started, the services and hosts it talks
to, and what the OS says about its access. Every fact is marked seen, inferred or unknown. Chats,
settings and tokens are never read.

Recognized products include Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider,
Cline, Goose, Docker Desktop, OrbStack, Podman Desktop and others.

Version 0.4.0 includes the child-tool classifications, expanded metadata and desktop agent
search described here. Upgrade from 0.3.0 or build current source to use them; the live demo
uses the current interface with sample data.

## From the CLI

```sh
holdmap agents                         # every agent and tool, with its footprint
holdmap agents claude                  # filter by product name or PID
holdmap agents claude --stop-ports --dry-run   # plan only
holdmap agents claude --stop-ports --yes       # stop unprotected ports that agent started
holdmap agents --json                  # machine-readable report
```

Each entry shows memory, CPU, process count, how many ports are stoppable, the parent process,
child processes grouped as tools & apps it started, folder source labels (working dir / child cwd /
recent), ports with roles, connections, and access facts.

Child tools include likely MCP servers even when they have no listening port. The report
includes evidence and omitted counts; an executable signature does not establish which tools
an agent called. Recent folders are current-account history, attached only to agents with a
matching known account, and do not establish current work by a particular running instance.

## Desktop and TUI

- **Desktop:** `⇧A` opens the Agents map. Expand a card to reveal a folder, open it in your editor,
  or stop the unprotected ports that agent started. Paths are verified against the current report
  before anything opens.
  Search by agent, folder, tool, exact PID (`pid:51200`) or explicit port (`port:3001`). A port
  search includes both the listener owner and agents connected to it.
- **TUI:** Tab cycles Ports → Graph → Agents. Select an agent and press `x` (or `X`) to stop its
  unprotected ports.

## Access and evidence

Access covers the account the process runs as, sandbox and approval flags, network listeners, and
(on macOS) protected folders. Each fact carries evidence: **seen**, **inferred** or **unknown**.
Unknown is labelled on cards so you never mistake a guess for a measurement.

## Safety

Stopping from the Agents surface only targets unprotected ports the agent started (dev servers and
services). The agent’s own IDE and auth listeners (`PortRole::Agent`) are skipped. Protected
processes with hard protection are never stopped. The CLI's `--allow-protected` flag can authorize
soft-protected targets only; MCP refuses all protected targets.

## Related

- [Desktop app](../desktop/): shortcuts and the Agents map.
- [MCP for AI agents](../mcp/): `list_agents` and `stop_agent_ports`.
- [CLI reference](../cli/): every flag for `holdmap agents`.
