<p align="center">
  <img src="apps/desktop/assets/icon.png" width="96" alt="portwise logo" />
</p>

<h1 align="center">portwise</h1>

<p align="center">
  <b>See which ports are in use, why they're busy, and stop the right thing safely.</b><br/>
  One Rust core, four surfaces: CLI · TUI · desktop/tray app · MCP server for AI agents.
</p>

<p align="center">
  <img src="docs/screenshots/desktop-light.png" width="860" alt="portwise desktop app" />
</p>

---

`EADDRINUSE: address already in use :::3000`. Most tools answer that with a PID and a `kill -9`.
portwise answers the real questions:

- **Who owns it?** Process, PID, user, command line, working directory, **project name, git branch
  and framework** (Next.js, Vite, SvelteKit, Django, FastAPI, Rails, Postgres, Redis…), or the
  **Docker / OrbStack / Colima / Podman container** that published it.
- **Why is it busy?** A plain-English explanation: a dev-server process tree (`npm → sh → node`), a
  container, a systemd / pm2 / Homebrew service, an OS feature (macOS AirPlay on 5000/7000, Windows
  HTTP.sys / excluded port ranges), `TIME_WAIT`, or a process owned by another user.
- **What should I do?** A previewable plan: stop the whole dev-server tree gracefully (SIGTERM →
  SIGKILL after a grace period), stop the container through its runtime, or run
  `brew services stop` / `systemctl --user stop`, and then **check that the port is actually free**.
- **What depends on it?** A live **service mesh**: which local services talk to which (web → api →
  db, cache), grouped into clusters (Compose project, Kubernetes namespace, pm2/turbo/nx parent,
  workspace, git repo), with port-forwards and tunnels (kubectl, `ssh -L`, cloudflared, ngrok).
  **Stop a whole cluster** in dependency order with one confirmed plan.

<p align="center">
  <img src="docs/screenshots/desktop-graph-light.png" width="860" alt="portwise desktop graph view" />
</p>

## Contents

- [Install](#install)
- [CLI](#cli)
- [TUI](#tui)
- [Desktop & tray app](#desktop--tray-app)
- [MCP server (AI agents)](#mcp-server-ai-agents)
- [Safety model](#safety-model)
- [Platform support](#platform-support)
- [Architecture](#architecture)
- [Development](#development)
- [License](#license)

## Install

### From source (works today)

```sh
# Rust 1.95+ (https://rustup.rs). macOS also needs the Xcode Command Line Tools (xcode-select --install).
git clone https://github.com/santoshshinde/portwise && cd portwise
cargo install --path crates/portwise-cli      # installs the `portwise` binary into ~/.cargo/bin
```

### Prebuilt binaries (once a release is tagged)

Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) (`.github/workflows/release.yml`):

```sh
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/santoshshinde/portwise/releases/latest/download/portwise-installer.sh | sh
# Homebrew
brew install santoshshinde/tap/portwise
# Windows (PowerShell)
powershell -c "irm https://github.com/santoshshinde/portwise/releases/latest/download/portwise-installer.ps1 | iex"
```

Desktop installers (`.dmg`, `.msi`/`.exe`, `.deb`/`.AppImage`) are built by `tauri-action`
(`.github/workflows/desktop-release.yml`) when a `desktop-v*` tag is pushed.

## CLI

```text
portwise                      Open the interactive TUI (when run in a terminal)
portwise list [QUERY]         Listening ports (alias: ls). --all for every socket, --dev, --mine, --udp, --json
portwise inspect 3000         Everything about a port: owner, process tree, project, plan
portwise explain 3000         Why is 3000 busy, and what should I do? (alias: why)
portwise stop 3000            Gracefully stop whatever holds 3000, then verify it's free
portwise stop 3000 --dry-run  Show the plan only
portwise stop pid:1234 vite   Stop by PID or by process name
portwise kill 3000            Force-kill now (SIGKILL / TerminateProcess)
portwise free-port --near 3000
portwise wait 5432 --timeout 30s        Wait until something is listening (or --free)
portwise run -p 3000 -- npm run dev     Free 3000 safely, then run the command with PORT=3000
portwise graph                Service mesh as a tree (alias: mesh). --json, --dot, --mermaid, --cluster NAME, --no-external
portwise stop --cluster acme-shop --dry-run   Stop a whole cluster in dependency order (also `stop cluster:acme-shop`)
portwise watch [--json]       Stream events: new / closed listeners, conflicts (NDJSON with --json)
portwise pin 3000 --label web Pin a port (shown first, watched even when free); unpin, pins
portwise history              What portwise stopped, with the command and cwd; `restart 3000` re-runs it
portwise open 3000            Open http://localhost:3000
portwise ssh devbox [graph]   Another machine's ports over SSH (agentless, read-only)
portwise mcp                  MCP server on stdio
portwise completions zsh      Shell completions (bash, zsh, fish, powershell, elvish)
portwise man                  Man page
```

`QUERY` understands `:3000` (exact), `3000-3999` (range), `proto:udp`, `pid:123`, `user:me` and free
text (`next`, `shop-web`, a branch name…).

<p align="center"><img src="docs/screenshots/cli-list.png" width="820" alt="portwise list" /></p>
<p align="center"><img src="docs/screenshots/cli-explain.png" width="820" alt="portwise explain 3000" /></p>
<p align="center"><img src="docs/screenshots/cli-stop.png" width="820" alt="portwise stop 8080 escalating to SIGKILL" /></p>
<p align="center"><img src="docs/screenshots/cli-graph.png" width="820" alt="portwise graph" /></p>
<p align="center"><img src="docs/screenshots/cli-stop-cluster-dry-run.png" width="820" alt="portwise stop --cluster --dry-run" /></p>

`portwise graph --mermaid` pastes straight into a GitHub comment, and `--dot | dot -Tsvg` renders it with Graphviz.

**Exit codes:** `0` ok · `1` busy / not found / timed out · `2` error · `3` blocked by the safety
policy · `4` needs elevation. Every command takes `--json` for scripting. Colour follows `--color`,
`NO_COLOR` and `CLICOLOR_FORCE`, and `portwise list | head` doesn't print "broken pipe" errors.

## TUI

`portwise` (or `portwise tui`) opens a keyboard-first terminal UI. It refreshes every 2 s, has
search, filters, sorting and a details pane (with PROCESS / PROJECT / NETWORK / PLAN sections) that sits on the right on wide terminals and below the
list on narrow ones. A grouped footer always shows the keys that matter right now.

<p align="center"><img src="docs/screenshots/tui.png" width="820" alt="portwise TUI" /></p>

| Key | Action | Key | Action |
|---|---|---|---|
| `↑↓` / `j k`, `g G` | move | `/` | search (same query language as the CLI) |
| `Enter` / `e` | explain | `x` / `Del` | stop gracefully, with confirmation |
| `X` | force kill, with confirmation | `o` / `c` | open in browser / copy URL (OSC 52) |
| `s` / `S` | sort / reverse | `t` | protocol: TCP+UDP → TCP → UDP |
| `d` / `m` / `a` | dev only / mine only / all sockets | `p` / `r` | pause / refresh |
| `Tab` / `v` | ports ⇄ graph tab (selection follows) | `C` | stop the selected service's cluster (dependency order) |
| `h` | graph: show / hide external hosts | `?` / `q` / `Esc` | help / quit |

<p align="center">
  <img src="docs/screenshots/tui-confirm-stop.png" width="410" alt="TUI stop confirmation" />
  <img src="docs/screenshots/tui-explain.png" width="410" alt="TUI explain view" />
</p>
<p align="center">
  <img src="docs/screenshots/tui-graph.png" width="410" alt="TUI graph tab" />
  <img src="docs/screenshots/tui-cluster-stop.png" width="410" alt="TUI cluster stop confirmation" />
</p>

## Desktop & tray app

A Tauri v2 + Svelte 5 app (`apps/desktop`), designed to feel like a native, keyboard-first tool
(think Linear or Raycast) while staying small: about 45 KB of gzipped JS and no UI framework beyond Svelte.

- **The port is the hero.** Each row leads with a large monospaced port number and a live status dot,
  then a framework tile (Next.js, Vite, FastAPI, Postgres, Redis… each with its own AA-contrast colour),
  then the project, git branch, PID and uptime. Clear badges mark *Exposed*, *Protected*, container runtime and other users' processes.
- **Command palette** (`⌘K` / `Ctrl K`). Fuzzy search over every port and action: jump to a port, stop or open it,
  find a free port, toggle filters, switch the theme or change the sort. The selected port ranks first.
- **Details panel.** A plain-English summary with a *Recommended* or *Blocked* callout, a
  **"What Stop will signal"** process-chain diagram (`npm run dev → sh → node`, with the holder marked),
  sections for process, project, container and network, and copyable terminal commands.
- **Quick stop you can trust.** `⌫` or the Stop button opens a confirmation listing the exact steps and their risk.
  Then you see live progress (step states, a grace-period bar, an activity log), a "Port N is free" success state, and a
  toast with a *Copy restart command* action. Selection moves to the next row, so you can keep going with the keyboard.
- **States that help.** Skeleton rows match the real layout, an animated radar empty state appears, searching a free
  port answers "Port 4321 is free" with a ready-to-copy `portwise run` command, and errors are actionable.
- **First-run hints**: a dismissable welcome card teaches `⌘K`, `/` and `⌫`.
- **Design system**: tokens for spacing, radius, type scale, colour and motion, plus light and dark themes checked for WCAG AA contrast
  (unit-tested). It has visible focus rings, ARIA roles (listbox, combobox, dialog, live regions) and `prefers-reduced-motion` support.
- **Responsive**: below 900 px the details pane becomes a slide-over drawer.
- **Graph view** (`g`). Services as cards grouped in cluster hulls, arrows from caller to callee,
  animated traffic dots, hover to highlight neighbours, selection shared with the list, zoom/fit,
  minimap, layered or force layout, light/dark, reduced motion respected. The details pane gains a
  *Connections* section and *Stop cluster* (`s`) shows the dependency-ordered plan.
- **Pins, history, restart.** Star a port (`p`) to keep it on top. The history panel (`h`) lists
  everything portwise stopped, with one-click restart.
- **Background helper.** Notifications for new or conflicting listeners, launch at login (starts
  hidden in the tray), and a global hotkey (`Ctrl+Alt+P` / `⌘⌥P`) to bring the window up. It re-scans
  every 4 s while visible and every 10 s while hidden.
- **Tray / menu-bar icon.** It shows "N dev servers running" and each one (`● :3000  Next.js · shop-web`), plus
  "N ports in use · M network-exposed". Closing the window keeps portwise in the tray.

| | |
|---|---|
| ![Light](docs/screenshots/desktop-light.png) | ![Dark](docs/screenshots/desktop-dark.png) |
| ![Command palette](docs/screenshots/desktop-command-palette.png) | ![Stop confirmation](docs/screenshots/desktop-stopping.png) |
| ![Stop success](docs/screenshots/desktop-stop-success.png) | ![Port freed toast](docs/screenshots/desktop-stopped-toast.png) |
| ![First run](docs/screenshots/desktop-onboarding.png) | ![Needs elevation](docs/screenshots/desktop-explain-blocked.png) |
| ![Free port answer](docs/screenshots/desktop-free-port.png) | ![Keyboard shortcuts](docs/screenshots/desktop-shortcuts.png) |
| ![Graph, dark](docs/screenshots/desktop-graph-dark.png) | ![Cluster stop plan](docs/screenshots/desktop-cluster-stop-plan.png) |
| ![List grouped by cluster](docs/screenshots/desktop-list-by-cluster.png) | ![Graph, light](docs/screenshots/desktop-graph-light.png) |

<p align="center"><img src="docs/screenshots/desktop-narrow.png" width="420" alt="Narrow window: details as a drawer" /></p>

<details>
<summary>Before and after the UI/UX polish pass</summary>

| Before | After |
|---|---|
| ![](docs/screenshots/before/desktop-light.png) | ![](docs/screenshots/desktop-light.png) |
| ![](docs/screenshots/before/desktop-dark.png) | ![](docs/screenshots/desktop-dark.png) |
| ![](docs/screenshots/before/desktop-stopping.png) | ![](docs/screenshots/desktop-stopping.png) |
| ![](docs/screenshots/before/tui.png) | ![](docs/screenshots/tui.png) |

</details>

Run it from source:

```sh
cd apps/desktop
npm install
npm run tauri dev          # dev mode with hot reload
npm run tauri build        # release build + installers for your OS
```

`npm run dev` on its own serves the UI in a normal browser with realistic mock data, which is handy for UI work.

## MCP server (AI agents)

`portwise mcp` speaks the Model Context Protocol over stdio and exposes these tools:

- `list_ports`
- `explain_port`
- `find_free_port`
- `wait_for_port`
- `stop_port`
- `get_topology`: the service graph and its clusters
- `plan_cluster_stop`: the dependency-ordered plan to stop a cluster (always a dry run)

`stop_port` supports `dry_run`. It refuses high-risk targets, and it needs `allow_non_dev` before it will touch anything that isn't a dev server. Example config for Claude Desktop, Cursor or VS Code:

```json
{ "mcpServers": { "portwise": { "command": "portwise", "args": ["mcp"] } } }
```

## Safety model

- **Graceful by default:** SIGTERM (Windows: `taskkill` without `/F`). Anything still running after
  the grace period (`--timeout`, default 5 s) gets SIGKILL / `TerminateProcess`.
- **Stop the right thing:** portwise climbs from the socket holder to the dev-server tree root
  (`npm`/`pnpm`/`yarn`/`bun`, `sh -c`, reloaders like `uvicorn --reload` and `nodemon`). If it
  only killed the child, the launcher would just respawn it.
- **Containers** are stopped through the runtime API, not by killing `com.docker.backend`.
  **Supervised services** go through their supervisor (`brew services`, `systemctl`, `pm2`).
- **Protected processes are never stopped by default.** One rule lives in `portwise-core`
  (`safety::protection`), so the CLI, TUI, desktop app and MCP server all behave the same:
  - *Hard-protected (never stopped):* PID 1, kernel threads, core OS processes (launchd, WindowServer,
    `System`, …), and portwise's **own process tree**: itself, its parents (your shell or editor), and
    anything it started (such as the desktop app's WebView helpers).
  - *Soft-protected (blocked unless you explicitly override):* system services and container daemons, plus
    **your sessions and whatever hosts them**: interactive shells, terminals, tmux, IDEs and IDE remote
    servers (`.vscode-server`, `.cursor-server`, …) and AI-agent hosts (Claude Code, Codex, Aider,
    agent daemons…). An ancestor of any of these is protected too, so stopping a "node" that turns out to
    be your editor's backend gets refused. Dev servers *started from* those sessions are not protected.
    To override, use `--allow-protected` in the CLI or *"I understand — stop anyway"* in the desktop app.
    The MCP server always refuses.
  - OS features like AirPlay Receiver and HTTP.sys get an explanation of how to turn them off instead.
- **PID-reuse guard:** every plan pins each process by its start time, and that is checked again right
  before the signal. Linux uses `pidfd_open` + `pidfd_send_signal`, so even a PID recycled in the
  microseconds before the signal can't be hit.
- **Dry-run everywhere:** `--dry-run`, the TUI/desktop confirm dialogs and MCP `dry_run` all show the
  same `ActionPlan`.
- **Verified outcome:** after stopping, portwise tries to bind the port to confirm it's free. It also
  reports when a supervisor or restart policy brings the port back.

## Platform support

| | Linux | macOS | Windows |
|---|---|---|---|
| Socket → PID | `/proc/net/{tcp,udp}{,6}` + `/proc/*/fd` inodes | libproc (`netstat2`) | `GetExtendedTcpTable`/`UdpTable` (`netstat2`) |
| Process identity / PID-reuse guard | `/proc/pid/stat` start time + **pidfd** | `proc_pidinfo(PROC_PIDTBSDINFO)` start time | `GetProcessTimes` creation time |
| Graceful stop | SIGTERM → SIGKILL | SIGTERM → SIGKILL | `taskkill` → `TerminateProcess` |
| Supervisors | systemd (service + socket units), pm2 | Homebrew services, pm2 | (Windows services are reported as protected) |
| OS-feature explanations | — | AirPlay Receiver (5000/7000) | HTTP.sys, excluded port ranges, WSL relay |
| Containers | Docker, Podman | Docker Desktop, OrbStack, Colima, Rancher, Podman | Docker Desktop (named pipe) |
| Status | **CI + real e2e tests** | type-checked; CI tests on `macos-latest` | type-checked; CI tests on `windows-latest` |

Sockets owned by other users are counted and explained ("hidden N sockets — run with sudo"), not silently dropped.

## Architecture

```text
crates/
  portwise-core/        the engine (no UI)
    provider.rs    Socket/Process/Container provider traits (+ Static* fakes for tests)
    scan.rs        Scanner: snapshot, v4/v6 grouping, container mapping, query language
    engine/        Engine (explain, plan, plan_cluster, topology) + strategies/ (one StopStrategy per owner kind)
    safety.rs      ProtectionPolicy          exec.rs  re-check identity → signal → escalate → verify
    project/       ProjectDetector, ManifestRegistry, workspace markers, git, framework signatures
    topology/      TopologyBuilder, ClusterRegistry, stop_order, exporters (tree/JSON/DOT/Mermaid)
    tunnel.rs remote.rs store.rs history.rs events.rs docker.rs sys/{linux,macos,windows}.rs
  portwise-cli/         `portwise` binary: clap CLI + ratatui TUI (src/tui)
  portwise-mcp/         MCP stdio server (JSON-RPC 2.0) on top of the core
apps/desktop/           Tauri v2 (src-tauri: commands/state/tray/watch/shortcuts) + Svelte 5 UI (src)
```

All four surfaces call the same `Engine`, so the CLI, TUI, desktop app and MCP server always agree
on what will happen. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for diagrams and the graph
library choice, and [docs/AUDIT.md](docs/AUDIT.md) for how each research point is covered.

## Development

```sh
cargo build                                   # core + CLI + MCP (default members)
cargo test                                    # unit + proptest + Linux e2e tests (real listeners, real signals)
cargo bench -p portwise-core                  # criterion: scan, topology, explain, parsers
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
scripts/demo-servers.sh start                 # realistic demo listeners on 3000/5173/8000/8080/8125
cd apps/desktop && npm install && npm run check && npm test && npm run tauri dev
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for conventions and [CHANGELOG.md](CHANGELOG.md) for history.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
