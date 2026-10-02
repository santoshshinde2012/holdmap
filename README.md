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
portwise mcp                  MCP server on stdio
portwise completions zsh      Shell completions (bash, zsh, fish, powershell, elvish)
portwise man                  Man page
```

`QUERY` understands `:3000` (exact), `3000-3999` (range), `proto:udp`, `pid:123`, `user:me` and free
text (`next`, `shop-web`, a branch name…).

<p align="center"><img src="docs/screenshots/cli-list.png" width="820" alt="portwise list" /></p>
<p align="center"><img src="docs/screenshots/cli-explain.png" width="820" alt="portwise explain 3000" /></p>
<p align="center"><img src="docs/screenshots/cli-stop.png" width="820" alt="portwise stop 8080 escalating to SIGKILL" /></p>

**Exit codes:** `0` ok · `1` busy / not found / timed out · `2` error · `3` blocked by the safety
policy · `4` needs elevation. Every command takes `--json` for scripting. Colour follows `--color`,
`NO_COLOR` and `CLICOLOR_FORCE`, and `portwise list | head` doesn't print "broken pipe" errors.

## TUI

`portwise` (or `portwise tui`) opens a keyboard-first terminal UI. It refreshes every 2 s, has
search, filters, sorting and a details pane that sits on the right on wide terminals and below the
list on narrow ones.

<p align="center"><img src="docs/screenshots/tui.png" width="820" alt="portwise TUI" /></p>

| Key | Action | Key | Action |
|---|---|---|---|
| `↑↓` / `j k`, `g G` | move | `/` | search (same query language as the CLI) |
| `Enter` / `e` | explain | `x` / `Del` | stop gracefully, with confirmation |
| `X` | force kill, with confirmation | `o` / `c` | open in browser / copy URL (OSC 52) |
| `s` / `S` | sort / reverse | `t` | protocol: TCP+UDP → TCP → UDP |
| `d` / `m` / `a` | dev only / mine only / all sockets | `p` / `r` | pause / refresh |
| `?` | help | `q` / `Esc` | quit |

<p align="center">
  <img src="docs/screenshots/tui-confirm-stop.png" width="410" alt="TUI stop confirmation" />
  <img src="docs/screenshots/tui-explain.png" width="410" alt="TUI explain view" />
</p>

## Desktop & tray app

A Tauri v2 + Svelte 5 app (`apps/desktop`). It includes:

- live list grouped into Dev servers / Containers / Databases / Apps / System
- search and filter chips: Listening ↔ All, TCP/UDP, Dev, Mine, Exposed
- a details pane with the explanation, process, project, container and network info, the exact stop plan, and copyable terminal commands
- one-click **Stop** / **Force kill** with a confirmation dialog that shows the plan and live progress, then a "port is free" toast
- light, dark and system themes, full keyboard control (`?` lists the shortcuts), empty, loading and error states, a "port N is free" answer when you search a free port, and ARIA roles, focus management and reduced-motion support
- a **tray / menu-bar icon** listing your running dev servers. Closing the window keeps portwise in the tray.

| Light | Dark |
|---|---|
| ![](docs/screenshots/desktop-light.png) | ![](docs/screenshots/desktop-dark.png) |
| ![](docs/screenshots/desktop-stopping.png) | ![](docs/screenshots/desktop-explain-blocked.png) |
| ![](docs/screenshots/desktop-stopped-toast.png) | ![](docs/screenshots/desktop-shortcuts.png) |

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
- **Protected processes are never stopped by default:** PID 1, kernel threads, portwise itself and
  its parents, and the OS (launchd, WindowServer, `svchost`, `System`, …) are hard-protected. Editors,
  terminals and system services are soft-protected; `--allow-protected` overrides them. OS features
  like AirPlay Receiver and HTTP.sys get an explanation of how to turn them off instead.
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
  portwise-core/        the engine (no UI): scan → resolve → explain → plan → execute
    sys/{linux,macos,windows}.rs   per-OS socket + process backends behind cfg
    scan.rs        snapshot: v4/v6 grouping, container mapping, filters/query language
    process.rs     process table, ancestry, tree roots
    project.rs     cwd → project root (package.json, Cargo.toml, pyproject, go.mod, …), git branch, framework signatures
    docker.rs      tiny sync HTTP client for Docker/Podman/OrbStack/Colima sockets & named pipes
    engine.rs      Owner resolution, Explanation, ActionPlan (with risk + blocked reasons)
    exec.rs        executes a plan: re-check identity → signal → escalate → verify free
    safety.rs      protected-process policy        probe.rs  bind probes
  portwise-cli/         `portwise` binary: clap CLI + ratatui TUI (src/tui)
  portwise-mcp/         MCP stdio server (JSON-RPC 2.0) on top of the core
apps/desktop/           Tauri v2 shell (src-tauri) + Svelte 5 UI (src)
```

All four surfaces call the same `Engine::explain`, `Engine::plan` and `execute` functions, so the CLI, TUI, desktop app and MCP server always agree on what will happen.

## Development

```sh
cargo build                                   # core + CLI + MCP (default members)
cargo test                                    # unit + Linux e2e tests (real listeners, real signals)
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
