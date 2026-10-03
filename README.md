<p align="center">
  <img src="apps/desktop/assets/icon.png" width="96" alt="portwise logo" />
</p>

<h1 align="center">portwise</h1>

<p align="center">
  <b>See which ports are in use, why they're busy, and stop the right thing safely.</b><br/>
  One Rust core, four surfaces: CLI · TUI · desktop and tray app · MCP server for AI agents.
</p>

<p align="center">
  <img src="docs/screenshots/desktop-overview-light.png" width="860" alt="The portwise desktop app: the port list grouped by kind, with the details pane for port 3000 open" />
</p>

---

`EADDRINUSE: address already in use :::3000`. Most tools answer with a PID and a `kill -9`.
portwise answers the questions you actually have:

- **Who owns it?** The process, PID, user, command line and working directory, plus the **project
  name, git branch and framework** (Next.js, Vite, SvelteKit, Django, FastAPI, Rails, Postgres,
  Redis…), or the **Docker, OrbStack, Colima or Podman container** that published it.
- **Why is it busy?** A plain-English explanation: a dev-server process tree (`npm → sh → node`),
  a container, a systemd, pm2 or Homebrew service, an OS feature (AirPlay Receiver on 5000/7000,
  Windows HTTP.sys or an excluded port range), `TIME_WAIT`, or another user's process.
- **What should I do?** A plan you can preview: stop the whole dev-server tree gracefully (SIGTERM,
  then SIGKILL after a grace period), stop the container through its runtime, or run
  `brew services stop` / `systemctl --user stop`. Afterwards portwise **checks the port is free**.
- **What depends on it?** A live **service graph** showing which local services talk to which
  (web → api → db), grouped into clusters (Compose project, Kubernetes namespace, pm2/turbo/nx
  parent, workspace, git repo), with port-forwards and tunnels (kubectl, `ssh -L`, cloudflared,
  ngrok). **Stop a whole cluster** in dependency order with one confirmed plan.

## Contents

- [Feature tour](#feature-tour)
- [Install](#install)
- [Quick start](#quick-start)
- [CLI](#cli)
- [TUI](#tui)
- [Desktop and tray app](#desktop-and-tray-app)
- [Settings and files](#settings-and-files)
- [MCP server for AI agents](#mcp-server-for-ai-agents)
- [Safety model](#safety-model)
- [FAQ and troubleshooting](#faq-and-troubleshooting)
- [Platform support](#platform-support)
- [Roadmap](#roadmap)
- [Documentation](#documentation)
- [Licence](#licence)

## Feature tour

**The list.** Every listening port, grouped into pinned ports, dev servers, containers, databases
and system services. Each row shows the framework, process, PID, git branch, badges (Exposed,
Protected, container runtime, another user), a CPU sparkline, memory and uptime.

| Light | Dark |
|---|---|
| ![Port list and details, light theme](docs/screenshots/desktop-overview-light.png) | ![Port list and details, dark theme](docs/screenshots/desktop-overview-dark.png) |

**Explain, then act.** Selecting a port explains who holds it and what Stop will do: which
processes get which signal, in what order, and the risk. Nothing happens until you confirm.

| Stop confirmation | Live progress |
|---|---|
| ![Stop confirmation listing the exact steps](docs/screenshots/desktop-stop-confirm-light.png) | ![Stop in progress with the grace-period bar](docs/screenshots/desktop-stop-progress-dark.png) |
| **Port freed** | **Toast with a restart command** |
| ![Success state: port 3001 is free](docs/screenshots/desktop-stop-success-light.png) | ![Toast after stopping, with Copy restart command](docs/screenshots/desktop-stopped-toast-dark.png) |

**Safety you can see.** Protected processes (your editor, terminal, AI agent host) are refused
unless you explicitly override, and sockets that need admin rights say so instead of failing.

| Protected process | Needs elevation |
|---|---|
| ![Can't stop VS Code safely, with an explicit override](docs/screenshots/desktop-stop-anyway-dark.png) | ![Port 631 is owned by root](docs/screenshots/desktop-explain-blocked-light.png) |

**The service graph.** Services as cards in cluster hulls, arrows from caller to callee, live
traffic, and a dependency-ordered cluster stop.

| Graph, light | Graph, dark |
|---|---|
| ![Service graph, light theme](docs/screenshots/desktop-graph-light.png) | ![Service graph, dark theme](docs/screenshots/desktop-graph-dark.png) |
| **List grouped by cluster** | **Cluster stop** |
| ![The list grouped by cluster](docs/screenshots/desktop-list-by-cluster-light.png) | ![Cluster stop refused because a member belongs to root](docs/screenshots/desktop-cluster-stop-light.png) |

**Keyboard first.** A command palette, single-key actions and a shortcut sheet.

| Command palette | Keyboard shortcuts |
|---|---|
| ![Command palette searching "sto"](docs/screenshots/desktop-command-palette-light.png) | ![Keyboard shortcuts dialog](docs/screenshots/desktop-shortcuts-dark.png) |

**And the rest.** Pins, a history of what you stopped with one-click restart, remote machines
over SSH, a free-port answer, settings, a compact density and a narrow-window layout.

| | |
|---|---|
| ![Pin a port with a label](docs/screenshots/desktop-pin-light.png) | ![Recently stopped, with Restart](docs/screenshots/desktop-history-light.png) |
| ![Remote host over SSH](docs/screenshots/desktop-remote-dark.png) | ![Searching a free port answers "Port 4321 is free"](docs/screenshots/desktop-free-port-light.png) |
| ![Settings, General](docs/screenshots/desktop-settings-light.png) | ![Settings, Appearance](docs/screenshots/desktop-settings-appearance-dark.png) |
| ![Compact density](docs/screenshots/desktop-compact-dark.png) | ![First-run welcome card](docs/screenshots/desktop-onboarding-light.png) |
| ![No ports match the search](docs/screenshots/desktop-no-match-light.png) | ![Narrow window: details as a sheet](docs/screenshots/desktop-narrow-light.png) |

**In the terminal.** The same engine drives the CLI and the TUI.

<p align="center"><img src="docs/screenshots/tui-list-dark.png" width="820" alt="The portwise TUI: port table with the details pane" /></p>

## Install

portwise is not published to package managers yet. Build it from source (it takes a minute or
two), or use the prebuilt installers once a release is tagged.

### macOS

```sh
xcode-select --install                     # once: Apple's command line tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust 1.95 or newer
git clone https://github.com/santoshshinde/portwise && cd portwise
cargo install --locked --path crates/portwise-cli   # installs `portwise` into ~/.cargo/bin
```

### Linux

```sh
# Debian/Ubuntu: a C toolchain. Fedora: sudo dnf install gcc
sudo apt-get install -y build-essential
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/santoshshinde/portwise && cd portwise
cargo install --locked --path crates/portwise-cli
```

### Windows

Install [Rust](https://rustup.rs) with the "Desktop development with C++" workload of the
Visual Studio Build Tools (rustup offers it), then in PowerShell:

```powershell
git clone https://github.com/santoshshinde/portwise; cd portwise
cargo install --locked --path crates/portwise-cli
```

### Prebuilt binaries (after the first tagged release)

Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) for macOS (Apple
silicon and Intel), Linux (x86-64 and ARM64) and Windows (x86-64):

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/santoshshinde/portwise/releases/latest/download/portwise-installer.sh | sh
# Homebrew
brew install santoshshinde/tap/portwise
# Windows (PowerShell)
powershell -c "irm https://github.com/santoshshinde/portwise/releases/latest/download/portwise-installer.ps1 | iex"
```

Desktop installers (`.dmg`, `.msi`/`.exe`, `.deb`/`.AppImage`) are attached to `desktop-v*`
releases. To build the desktop app yourself, see [Desktop and tray app](#desktop-and-tray-app).

### Shell completions and man pages

```sh
portwise completions bash > ~/.local/share/bash-completion/completions/portwise
portwise completions zsh  > ~/.zfunc/_portwise        # with fpath+=(~/.zfunc) in ~/.zshrc
portwise completions fish > ~/.config/fish/completions/portwise.fish
portwise completions powershell >> $PROFILE           # PowerShell
portwise man --out-dir ~/.local/share/man/man1        # portwise.1 plus one page per command
```

## Quick start

```sh
portwise                     # open the interactive TUI
portwise list --dev          # just the dev servers
portwise explain 3000        # who holds 3000, why, and what to do about it
portwise stop 3000 --dry-run # the exact plan, without doing anything
portwise stop 3000           # stop it gracefully and check the port is free
portwise run -p 3000 -- npm run dev   # free 3000 safely, then start your server on it
```

## CLI

Every command, flag and default is in the generated **[CLI reference](docs/cli.md)**. The
highlights:

| Command | What it does |
|---|---|
| `portwise` | Opens the TUI when run in a terminal (also `portwise tui`) |
| `portwise list [QUERY]` (alias `ls`) | Listening ports. `--all` every socket, `--dev`, `--mine`, `--exposed`, `--tcp`, `--udp`, `--range 3000-3999`, `--sort memory`, `--wide`, `--json` |
| `portwise inspect 3000` | Everything about a port: owner, process tree, project, plan |
| `portwise explain 3000` (alias `why`) | Why is 3000 busy, and what should I do? |
| `portwise stop 3000` | Gracefully stop whatever holds 3000, then check it is free. `--dry-run`, `--yes`, `--timeout 10s`, `--no-tree`, `--allow-protected` |
| `portwise stop pid:1234 vite` | Stop by PID or process name (also `--pid`, `--name`) |
| `portwise stop --cluster shop` | Stop a whole cluster, dependents first (also `stop cluster:shop`) |
| `portwise kill 3000` | Force-kill now (SIGKILL / TerminateProcess), same as `stop --force` |
| `portwise free-port --near 3000` | Print a free TCP port (`--range`, `--count 3`) |
| `portwise wait 5432 --timeout 30s` | Wait until something accepts connections (`--free` waits until it's free) |
| `portwise run -p 3000 -- npm run dev` | Free the port safely, then run the command with `PORT=3000` (`--fallback` picks the next free port instead) |
| `portwise graph` (alias `mesh`) | The service graph as a tree. `--json`, `--dot`, `--mermaid`, `--cluster NAME`, `--no-external`, `--all` |
| `portwise watch` | Stream new, closed and conflicting listeners (`--json` for NDJSON) |
| `portwise pin 3000 --label web` | Pin a port: shown first and watched even when free |
| `portwise unpin 3000` | Remove a pin |
| `portwise pins` | List pinned ports and whether they're in use |
| `portwise history` | What portwise stopped, with the command and directory (`--clear`) |
| `portwise restart 3000` | Stop what holds 3000 and start the same command again, or re-run it from history |
| `portwise open 3000` | Open `http://localhost:3000` (`--print` just prints the URL) |
| `portwise ssh devbox [graph]` | Another machine's ports over SSH: agentless and read-only |
| `portwise mcp` | The MCP server on stdio, for AI agents |
| `portwise completions zsh` | Shell completions: bash, zsh, fish, powershell, elvish |
| `portwise man` | The man page (`--out-dir DIR` writes one page per command) |

**Queries.** `list` and the search boxes understand `:3000` (exact port), `3000-3999` (range),
`proto:udp`, `pid:123`, `user:me` and free text (`next`, `shop-web`, a branch name).

**Scripting.** Every read command takes `--json`. Exit codes are `0` ok, `1` busy, not found or
timed out, `2` error, `3` blocked by the safety policy, and `4` needs elevation. Colour follows
`--color auto|always|never` (or `PORTWISE_COLOR`), `NO_COLOR` and `CLICOLOR_FORCE`. `--no-docker`
skips the container runtimes. `portwise list | head` doesn't print "broken pipe" errors.

| `portwise list --dev 3000-9999` | `portwise explain 3000` |
|---|---|
| ![portwise list](docs/screenshots/cli-list-dark.png) | ![portwise explain 3000](docs/screenshots/cli-explain-dark.png) |
| **`portwise stop 3000 --dry-run`** | **`portwise stop 8080 --yes` (escalates to SIGKILL)** |
| ![portwise stop --dry-run](docs/screenshots/cli-stop-dry-run-dark.png) | ![portwise stop escalating to SIGKILL](docs/screenshots/cli-stop-dark.png) |
| **`portwise graph`** | **`portwise stop --cluster acme-shop --dry-run`** |
| ![portwise graph](docs/screenshots/cli-graph-dark.png) | ![portwise stop --cluster --dry-run](docs/screenshots/cli-stop-cluster-dry-run-dark.png) |

`portwise graph --mermaid` pastes straight into a GitHub comment, and `portwise graph --dot | dot -Tsvg > graph.svg` renders it with Graphviz:

<p align="center"><img src="docs/screenshots/cli-graph-mermaid-dark.png" width="640" alt="portwise graph --mermaid output" /></p>

## TUI

`portwise` (or `portwise tui`) opens a keyboard-first terminal UI. It refreshes every 2 seconds
and has search, filters, sorting and a details pane (process, project, network and plan) that sits
on the right on wide terminals and below the list on narrow ones. The footer always shows the keys
that matter right now, and `?` lists them all.

| Key | Action |
|---|---|
| `↑` `↓` / `j` `k`, `PgUp` `PgDn`, `Home`/`g`, `End`/`G` | Move the selection |
| `/` | Search, with the same query language as the CLI (`Ctrl-U` clears) |
| `Enter` / `e` | Explain the selected port |
| `x` / `Del` / `Backspace` | Stop gracefully, after a confirmation that shows the plan |
| `X` | Force kill, after confirmation |
| `o` / `c` | Open in the browser / copy the URL (OSC 52, works over SSH) |
| `s` / `S` | Change the sort / reverse it |
| `t` | Protocol: TCP+UDP → TCP → UDP |
| `d` / `m` / `a` | Dev servers only / mine only / all sockets |
| `p` / `r` or `F5` | Pause / refresh now |
| `Tab` / `Shift-Tab` / `v` | Switch between the ports and graph tabs (the selection follows) |
| `C` | Graph tab: stop the selected service's cluster, in dependency order |
| `h` | Graph tab: show or hide external hosts |
| `?` / `F1` | Help |
| `q` / `Esc` | Quit (or close the open dialog) |

| Explain | Stop confirmation |
|---|---|
| ![TUI explain view](docs/screenshots/tui-explain-dark.png) | ![TUI stop confirmation](docs/screenshots/tui-confirm-stop-dark.png) |
| **Graph tab** | **Cluster stop** |
| ![TUI graph tab](docs/screenshots/tui-graph-dark.png) | ![TUI cluster stop confirmation](docs/screenshots/tui-cluster-stop-dark.png) |

<p align="center"><img src="docs/screenshots/tui-help-dark.png" width="640" alt="TUI help overlay" /></p>

## Desktop and tray app

A Tauri v2 and Svelte 5 app in `apps/desktop`. It's keyboard-first, lives in the tray or menu bar
when its window is closed, and ships about 160 KB of gzipped JavaScript with bundled Inter and
JetBrains Mono fonts.

- **List** grouped by kind or by cluster, with search, filters (dev, mine, exposed, protocol, all
  sockets) and sorting. Sections collapse, and a compact density fits more rows.
- **Details pane** with Overview, Connections, Process, Network and Commands tabs, a
  "What Stop will signal" process chain, and copyable CLI equivalents. In narrow windows it
  becomes a sheet.
- **Stop flow**: confirmation with the exact steps, live progress, a "Port N is free" state, and
  a toast with *Copy restart command*. Selection moves to the next row.
- **Graph view** (`g`): cluster hulls, directed edges with traffic, hover highlighting, a minimap,
  layered or force layout, and cluster stop.
- **Pins, history and restart**, a **remote view** of another machine over SSH, and a
  **free-port answer** when you search for a port nobody holds.
- **Background helper**: notifications for new and conflicting listeners, launch at login (starts
  hidden in the tray), and a global shortcut to bring the window back.
- **Tray menu**: "N dev servers running" with one item per server, "N ports in use · M
  network-exposed", Refresh and Quit.

### Keyboard shortcuts

`⌘` on macOS is `Ctrl` on Linux and Windows. `?` shows this list in the app.

| Key | Action | Key | Action |
|---|---|---|---|
| `⌘K` | Command palette | `/` or `⌘F` | Search |
| `↑` `↓` / `j` `k` | Move the selection | `←` `→` | Collapse / expand the section |
| `Home` `End`, `PgUp` `PgDn` | Jump in the list | `Enter` | Open the details |
| `Esc` | Clear the search, close a dialog | `⌘R` or `r` | Refresh now |
| `⌫` | Stop the selected port (graceful) | `⇧⌫` | Force kill |
| `o` | Open in the browser | `c` | Copy the URL |
| `p` | Pin / unpin | `⇧P` | Pin with a label… |
| `s` | Stop the service's cluster | `g` | List ⇄ graph |
| `a` | Listening ⇄ all sockets | `t` | Protocol: any → TCP → UDP |
| `d` | Dev servers only | `m` | Mine only |
| `e` | Network-exposed only | `h` | Recently stopped |
| `⌘,` | Settings | `⇧L` | Cycle the theme |
| `?` | Keyboard shortcuts | `⌘⌥P` / `Ctrl+Alt+P` | Show portwise from anywhere (configurable) |

### Build and run

```sh
cd apps/desktop
npm install
npm run tauri dev       # the app with hot reload
npm run tauri build     # a release build plus installers for your OS
npm run dev             # just the UI in a browser at http://localhost:1420, with mock data
```

Linux needs the WebKitGTK and tray libraries first:
`sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`.
Windows needs WebView2, which Windows 10 and 11 already include.

## Settings and files

The desktop app's Settings (`⌘,`) has five sections: General (launch at login, global shortcut),
Appearance (theme, list density, reduced motion follows the system), Notifications, Scanning & data
(scan interval, history size) and About. Changes save immediately.

| Setting | Default | Notes |
|---|---|---|
| Scan interval | 4 s | 1–60 s while the window is open. Hidden, the app scans every 2.5× that, at least every 10 s |
| History size | 200 | 10–5000 stopped ports remembered for restart |
| Notifications | on, dev servers only | New and conflicting listeners, and pinned ports opening or closing |
| Global shortcut | `⌘⌥P` / `Ctrl+Alt+P` | Or `⌥Space` / `Ctrl+Alt+Space`, `⌘⌥K` / `Ctrl+Alt+K`, or off |
| Theme, density | system, comfortable | Stored by the UI; `⇧L` cycles the theme |

The CLI, TUI and desktop app share one configuration directory:

| Platform | Directory |
|---|---|
| Linux | `$XDG_CONFIG_HOME/portwise` (usually `~/.config/portwise`) |
| macOS | `~/Library/Application Support/portwise` |
| Windows | `%APPDATA%\portwise` |
| Any | `$PORTWISE_HOME` overrides the above |

It holds `config.json` (pins, notification and scan settings, shortcut, recent SSH hosts),
`history.jsonl` (stopped ports, newest last) and `logs/` (output of restarted commands).

## MCP server for AI agents

`portwise mcp` speaks the [Model Context Protocol](https://modelcontextprotocol.io) over stdio.
Agents can list ports, ask why one is busy, find a free port, wait for a server, read the service
graph and stop their own dev servers safely.

| Tool | Arguments | Notes |
|---|---|---|
| `list_ports` | `query`, `dev_only`, `protocol` | Read-only |
| `explain_port` | `port` | Root cause, recommended fix and plan |
| `find_free_port` | `near` | Read-only |
| `wait_for_port` | `port`, `timeout_s` (30), `until_free` | Read-only |
| `get_topology` | `all`, `format` (`json`, `mermaid`, `dot`, `tree`) | Read-only |
| `plan_cluster_stop` | `cluster` | Always a dry run |
| `stop_port` | `port`, `dry_run`, `force`, `allow_non_dev` | Low-risk owners only unless `allow_non_dev`; protected and other users' processes are always refused |

**Claude Desktop** (`claude_desktop_config.json`) and **Cursor** (`~/.cursor/mcp.json` or
`.cursor/mcp.json` in a project):

```json
{
  "mcpServers": {
    "portwise": { "command": "portwise", "args": ["mcp"] }
  }
}
```

**VS Code** (`.vscode/mcp.json` in a workspace, or *MCP: Open User Configuration*):

```json
{
  "servers": {
    "portwise": { "type": "stdio", "command": "portwise", "args": ["mcp"] }
  }
}
```

If the client can't find `portwise`, use the full path (`~/.cargo/bin/portwise`, or
`%USERPROFILE%\.cargo\bin\portwise.exe` on Windows). [docs/mcp.md](docs/mcp.md) has the details,
config file locations and example sessions.

## Safety model

portwise kills processes, so the rules are strict and live in one place (`portwise-core`). The CLI,
TUI, desktop app and MCP server all behave the same.

- **Graceful by default.** SIGTERM (on Windows, `taskkill` without `/F`, which sends `WM_CLOSE`).
  Anything still running after the grace period (`--timeout`, default 5 s) gets SIGKILL or
  `TerminateProcess`.
- **Stop the right thing.** portwise climbs from the socket holder to the dev-server tree root
  (`npm`/`pnpm`/`yarn`/`bun`, `sh -c`, reloaders like `uvicorn --reload` and `nodemon`). Killing
  only the child would let the launcher respawn it.
- **Owners, not PIDs.** Containers are stopped through the runtime API, not by killing
  `com.docker.backend`. Supervised services go through their supervisor (`brew services`,
  `systemctl`, `pm2`).
- **Protected processes are never stopped by default.**
  - *Hard-protected, never stopped:* PID 1, kernel threads, core OS processes (launchd,
    WindowServer, `System`…) and portwise's own process tree: itself, its parents (your shell or
    editor) and anything it started.
  - *Soft-protected, blocked unless you override:* system services and container daemons, plus your
    sessions and whatever hosts them: interactive shells, terminals, tmux, IDEs and their remote
    servers (`.vscode-server`, `.cursor-server`…) and AI-agent hosts (Claude Code, Codex, Aider…).
    Dev servers *started from* those sessions are not protected. Override with `--allow-protected`
    in the CLI or *"I understand — stop it anyway"* in the desktop app. The MCP server always refuses.
  - OS features such as AirPlay Receiver and HTTP.sys get instructions for turning them off instead.
- **PID-reuse guard.** Every plan pins each process by its start time and checks it again just
  before signalling. On Linux, `pidfd_open` and `pidfd_send_signal` make that race-free.
- **Dry run everywhere.** `--dry-run`, the TUI and desktop confirmations and MCP `dry_run` all show
  the same `ActionPlan`.
- **Verified outcome.** After stopping, portwise binds the port to confirm it's free, and it tells
  you when a supervisor or restart policy brings the port back.
- **Local and private.** No telemetry and no network access beyond the local container runtime
  socket and SSH connections you ask for.

## FAQ and troubleshooting

<details>
<summary><b>A port shows "owned by root" or "N hidden sockets". Why can't portwise see it?</b></summary>

Operating systems only show you other users' sockets with admin rights. portwise counts them
instead of hiding them. Run `sudo portwise explain PORT` (Linux and macOS) or an elevated terminal
on Windows to see and stop them. Exit code `4` means elevation is needed.
</details>

<details>
<summary><b>portwise refuses to stop a process (exit code 3).</b></summary>

It's protected: your editor's backend, a terminal, an agent host, a system service or portwise
itself. `portwise explain PORT` says which rule applied. If you're sure, use
`portwise stop PORT --allow-protected` (soft protection only).
</details>

<details>
<summary><b>I stopped it and the port came back.</b></summary>

Something restarts it: a supervisor (systemd, launchd, pm2, `brew services`), a container restart
policy, or a file watcher. portwise reports this after the stop. Stop the supervisor's unit (for
example `systemctl --user stop NAME` or `pm2 stop NAME`), which `explain` suggests when it detects
one.
</details>

<details>
<summary><b>Port 5000 or 7000 is busy on macOS and nothing of mine is running.</b></summary>

That's AirPlay Receiver. Turn it off in System Settings → General → AirDrop & Handoff → AirPlay
Receiver, or use another port. `portwise explain 5000` says the same.
</details>

<details>
<summary><b>Windows says the port is in use, but nothing is listening.</b></summary>

It's probably inside an excluded port range reserved by Hyper-V, WSL or Docker (`portwise explain`
shows the range), or held by HTTP.sys. Pick a port outside the range (`portwise free-port --near
PORT`) or see `netsh interface ipv4 show excludedportrange protocol=tcp`.
</details>

<details>
<summary><b>Containers aren't detected.</b></summary>

portwise looks for `DOCKER_HOST`, then the Docker Desktop, OrbStack, Colima, Rancher Desktop and
Podman sockets. Check that your user can read the socket (`docker ps` works without sudo). Use
`--no-docker` to skip containers entirely.
</details>

<details>
<summary><b>The MCP client says "command not found".</b></summary>

GUI apps don't always inherit your shell's `PATH`. Put the absolute path to the binary in
`command`. See [docs/mcp.md](docs/mcp.md#troubleshooting).
</details>

<details>
<summary><b>The global shortcut doesn't work on Linux.</b></summary>

Global shortcuts need X11 or a compositor that allows them; many Wayland sessions don't. Pick
another preset in Settings, or use the tray icon.
</details>

<details>
<summary><b>How do I reset everything?</b></summary>

Delete the configuration directory listed in [Settings and files](#settings-and-files), or run
`portwise history --clear` to forget only the history.
</details>

## Platform support

| | Linux | macOS | Windows |
|---|---|---|---|
| Socket → PID | `/proc/net/{tcp,udp}{,6}` and `/proc/*/fd` inodes | libproc (`netstat2`) | `GetExtendedTcpTable` / `GetExtendedUdpTable` (`netstat2`) |
| Process identity and PID-reuse guard | `/proc/PID/stat` start time and **pidfd** | `proc_pidinfo` start time | `GetProcessTimes` creation time |
| Graceful stop | SIGTERM → SIGKILL | SIGTERM → SIGKILL | `taskkill` (WM_CLOSE) → `TerminateProcess` |
| Supervisors | systemd (service and socket units), pm2 | Homebrew services, pm2 | Windows services are reported as protected |
| OS-feature explanations | — | AirPlay Receiver (5000/7000) | HTTP.sys, excluded port ranges, WSL relay |
| Containers | Docker, Podman | Docker Desktop, OrbStack, Colima, Rancher Desktop, Podman | Docker Desktop (named pipe) |
| CLI, TUI, MCP | ✅ | ✅ | ✅ |
| Desktop and tray app | ✅ (the tray needs AppIndicator) | ✅ | ✅ |
| Testing | CI plus end-to-end tests with real processes | CI on `macos-latest` | CI on `windows-latest` |

## Roadmap

Planned or under consideration (see [docs/audit.md](docs/audit.md) for the full matrix):

- Signed and notarised releases, and publishing to Homebrew, winget and Scoop.
- A privileged helper so the desktop app can show and stop other users' sockets.
- `CTRL_BREAK` for Windows console apps, and stopping Windows services and launchd jobs through
  their managers.
- A `.portwise.toml` project file for named ports, and a shell hook that explains `EADDRINUSE`.
- Editor and launcher extensions (VS Code, Raycast) on top of the JSON and MCP interfaces.
- Detecting port drift (5173 → 5174) and persisting the event log.

## Documentation

| Document | What's in it |
|---|---|
| [User guide](docs/user-guide.md) | Task-by-task walkthrough of the CLI, TUI and desktop app |
| [CLI reference](docs/cli.md) | Every command and flag, generated from the code |
| [MCP guide](docs/mcp.md) | Client setup, tools, safety and troubleshooting |
| [Architecture](docs/architecture.md) | How the core and the four surfaces fit together |
| [Research audit](docs/audit.md) | Every research point and its status in the code |
| [Development](docs/development.md) | Building, testing, demo servers and screenshots |
| [Releasing](docs/releasing.md) | Versioning and the release workflows |
| [Contributing](CONTRIBUTING.md) | Ground rules, checks, naming and commit conventions |
| [Security policy](SECURITY.md) | Reporting vulnerabilities |
| [Changelog](CHANGELOG.md) | What changed in each version |

## Licence

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option. Unless you explicitly state otherwise, any contribution
you intentionally submit for inclusion in this work, as defined in the Apache-2.0 licence, is dual
licensed as above, without any additional terms or conditions.

The desktop app bundles Inter and JetBrains Mono under the SIL Open Font License 1.1 (see
`apps/desktop/src/assets/fonts/`).
