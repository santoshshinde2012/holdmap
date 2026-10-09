---
title: Getting started
description: Install holdmap, see who's on a port or which agent started it, and stop the right thing safely.
order: 1
---

holdmap answers three questions about any port: **who** is on it, **why** it's busy, and **how** to
stop it without collateral damage — and shows which **agents, tools and apps** started those ports.
The CLI, the terminal UI, the desktop app and the MCP server all ask the same Rust core, so they
always agree.

## Install

**macOS and Linux**

```sh
curl -LsSf https://github.com/santoshshinde2012/holdmap/releases/latest/download/holdmap-installer.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/santoshshinde2012/holdmap/releases/latest/download/holdmap-installer.ps1 | iex"
```

The binary goes to `~/.local/bin` (`%USERPROFILE%\.local\bin` on Windows). If that folder isn't on
your `PATH` yet, the installer adds it: open a new terminal, or run the `source` line it prints.
The desktop app is a separate download, see [Install](../#install).

## Your first minute

```sh
holdmap list                 # every listening port, grouped by kind
holdmap agents               # AI agents and tools, with folders / ports / access
holdmap explain 3000         # who holds it, why, and what to do
holdmap inspect 3000         # owner, process tree, connections, bind risk and the stop plan
holdmap stop 3000 --dry-run  # show exactly what stop would do
holdmap stop 3000            # stop it gracefully, then check the port is free
holdmap                      # the interactive TUI (Tab → Agents)
```

`stop` always shows the plan and asks before it acts. It stops the whole dev-server tree (so `npm`
or `nodemon` can't respawn the child), sends SIGTERM first and SIGKILL only after a timeout, and then
checks that the port really is free.

## Everyday recipes

| You want to… | Run |
|---|---|
| Free a port and start your server on it | `holdmap run -p 3000 -- npm run dev` |
| Find the next free port | `holdmap free-port --near 3000` |
| Wait for a database in a script | `holdmap wait 5432 --timeout 30s` |
| See the heaviest apps first | `holdmap list --sort memory` |
| Only your dev servers | `holdmap list --dev` |
| Ports open to the network | `holdmap list --exposed` |
| Which services talk to which | `holdmap graph` |
| Which agents and tools are running | `holdmap agents` |
| Stop ports one agent started | `holdmap agents claude --stop-ports --dry-run` |
| Another machine's ports | `holdmap ssh devbox` |

Every read command takes `--json`. Exit codes: `0` ok, `1` busy, not found or timed out, `2` error,
`3` blocked by the safety policy, `4` needs elevation.

## Next steps

- [Agents, tools and apps](agents/): folders, access, and stop what an agent started.
- [Project stacks](projects/): start and stop a project's services with `holdmap up` and `down`.
- [Shell hook](shell-hook/): explain "port already in use" the moment it happens.
- [MCP for AI agents](mcp/): let Claude, Cursor or VS Code read and free ports safely.
- [Desktop app](desktop/): shortcuts, the service graph, the agents map and the first open.
- [CLI reference](cli/): every command and flag.
