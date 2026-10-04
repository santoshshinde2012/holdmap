<p align="center">
  <img src="apps/desktop/assets/icon.png" width="88" alt="portwise logo" />
</p>

<h1 align="center">portwise</h1>

<p align="center">
  <b>See which ports are in use, why they're busy, and stop the right thing safely.</b><br/>
  CLI · TUI · desktop and tray app · MCP server, all on one Rust core.
</p>

<p align="center">
  <a href="https://github.com/santoshshinde2012/portwise/actions/workflows/ci.yml"><img src="https://github.com/santoshshinde2012/portwise/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI" /></a>
  <a href="https://github.com/santoshshinde2012/portwise/releases/latest"><img src="https://img.shields.io/github/v/release/santoshshinde2012/portwise" alt="Latest release" /></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey" alt="Platforms: macOS, Linux, Windows" />
</p>

<p align="center">
  <img src="docs/screenshots/desktop-overview-light.png" width="860" alt="The portwise desktop app: ports grouped by kind, with the details pane for port 3000 open" />
</p>

`EADDRINUSE: address already in use :::3000`? Most tools hand you a PID and a `kill -9`.
portwise tells you what's really there and stops it properly.

## Features

- **Who owns the port.** Process, user, command, folder, plus the project, git branch and
  framework (Next.js, Vite, Django, Postgres…) or the Docker / Podman / OrbStack container.
- **Why it's busy, in plain English.** A dev-server tree, a container, a systemd / pm2 / Homebrew
  service, an OS feature like AirPlay Receiver, `TIME_WAIT`, or another user.
- **A safe stop.** Preview the plan, stop the whole dev-server tree gracefully (SIGTERM, then
  SIGKILL), and check the port is free afterwards. System processes and IDEs are protected.
- **The service graph.** Which local services talk to which, grouped into clusters (Compose,
  Kubernetes, workspaces). Stop a whole stack in dependency order.
- **Everywhere you work.** A scriptable CLI (`--json`), a TUI, a desktop and tray app, and an MCP
  server for AI coding assistants. macOS, Linux and Windows. No telemetry.

## Install

**macOS and Linux**

```sh
curl -LsSf https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise-installer.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise-installer.ps1 | iex"
```

The binary goes to `~/.local/bin` (`%USERPROFILE%\.local\bin` on Windows). If that folder is
already on your `PATH`, `portwise` works right away. If not, the installer adds it and tells
you: open a new terminal, or run the `source` line it prints.

**Desktop app**

| Platform | Download |
|---|---|
| macOS, Apple silicon | [portwise_aarch64.dmg][dmg-arm64] |
| macOS, Intel | [portwise_x64.dmg][dmg-x64] |
| Windows | [.msi][msi] or [setup .exe][nsis] |
| Linux | [.AppImage][appimage], [.deb][deb] or [.rpm][rpm] |

<!-- release-please bumps these (one version per line; the rpm's "-" is %2D so its "-1" release
     suffix isn't read as part of the version). -->
<!-- x-release-please-start-version -->
[dmg-arm64]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_aarch64.dmg
[dmg-x64]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_x64.dmg
[msi]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_x64_en-US.msi
[nsis]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_x64-setup.exe
[appimage]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_amd64.AppImage
[deb]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise_0.1.2_amd64.deb
[rpm]: https://github.com/santoshshinde2012/portwise/releases/latest/download/portwise-0.1.2%2D1.x86_64.rpm
<!-- x-release-please-end -->

**Homebrew:** coming soon.

**From source** (Rust 1.95+): `cargo install --locked --git https://github.com/santoshshinde2012/portwise portwise`

Every release has SHA-256 checksums and build provenance (`gh attestation verify FILE --repo santoshshinde2012/portwise`).

## Quick start

```sh
portwise list                 # every listening port, grouped by kind
portwise inspect 3000         # owner, process tree, project and the stop plan
portwise stop 3000 --dry-run  # show exactly what stop would do
portwise stop 3000            # stop it gracefully, then check the port is free
portwise run -p 3000 -- npm run dev   # free 3000 safely, then start your server on it
portwise                      # the interactive TUI
```

<img src="docs/screenshots/cli-explain-dark.png" width="720" alt="portwise explain 3000: who holds the port, why, and the recommended plan" />

**Project stacks.** Describe a project's services once in `.portwise.toml` (`portwise init` writes
one from what's running) and start them in dependency order with `portwise up`:

```toml
name = "shop"

[services.api]
port = 4000
command = "npm run dev"   # runs with PORT set
depends_on = ["db"]

[services.db]
port = 5432               # no command: started elsewhere, up just waits for it
```

`portwise status` shows the stack and `portwise down` stops it, dependents first.

## CLI

| Purpose | Commands |
|---|---|
| Look | `portwise list`, `portwise inspect`, `portwise explain`, `portwise graph`, `portwise watch`, `portwise ssh HOST` (read-only, nothing to install remotely) |
| Act | `portwise stop`, `portwise kill`, `portwise restart`, `portwise run`, `portwise open` |
| Ports | `portwise free-port --near 3000`, `portwise wait 5432 --timeout 30s` |
| Projects | `portwise up`, `portwise down`, `portwise status`, `portwise init` |
| Remember | `portwise pin`, `portwise unpin`, `portwise pins`, `portwise history` |
| Integrate | `portwise tui`, `portwise mcp`, `portwise completions zsh`, `portwise man` |

Every read command takes `--json`. Exit codes: `0` ok, `1` busy / not found / timed out, `2`
error, `3` blocked by the safety policy, `4` needs elevation. All flags are in the
**[CLI reference](docs/cli.md)**.

**Shell hook.** Add `eval "$(portwise init zsh)"` to `~/.zshrc` (also `bash`, `fish`, `pwsh`).
When a command fails with "port in use", it prints who holds the port and how to free it.

## Desktop app

A tray and menu-bar app with the port list, details, the service graph (`G`), pins, history with
one-click restart, remote hosts over SSH and a command palette (`⌘K` / `Ctrl+K`). Press `?` for
every shortcut.

**First open.** The app isn't notarised yet, so the OS asks once:

- **macOS:** open it once, then go to **System Settings > Privacy & Security > Open Anyway**
  (macOS 14 and earlier: right-click the app, then **Open**). Or run
  `xattr -dr com.apple.quarantine /Applications/portwise.app`.
- **Windows:** in "Windows protected your PC", click **More info**, then **Run anyway**.

<details>
<summary>More screenshots</summary>

| Stop confirmation | Service graph |
|---|---|
| ![The stop confirmation lists every step before anything happens](docs/screenshots/desktop-stop-confirm-light.png) | ![The service graph with a Compose cluster, dark theme](docs/screenshots/desktop-graph-dark.png) |
| **Dark theme** | **Command palette** |
| ![Port list and details, dark theme](docs/screenshots/desktop-overview-dark.png) | ![The command palette searching "sto"](docs/screenshots/desktop-command-palette-light.png) |
| **TUI** | |
| ![The portwise TUI: port table with the details pane](docs/screenshots/tui-list-dark.png) | |

</details>

## MCP server

`portwise mcp` lets AI coding assistants list ports, explain them, find a free port, wait for a
server, read the service graph and stop their own dev servers (never protected processes).
For Claude Desktop or Cursor (`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "portwise": { "command": "portwise", "args": ["mcp"] }
  }
}
```

VS Code (`.vscode/mcp.json`) uses `"servers"` with `"type": "stdio"`. If the client can't find
`portwise`, use the full path, for example `/Users/you/.local/bin/portwise`.

## Safety model

portwise stops processes, so one set of rules in `portwise-core` applies to every surface:

- **Graceful first:** SIGTERM (`taskkill` on Windows), then a force kill after `--timeout` (5 s).
- **The right target:** the dev-server tree root (`npm`, `nodemon`, `uvicorn --reload`…) so nothing
  respawns; containers through their runtime, services through their supervisor.
- **Protected processes:** the OS, shells, terminals, IDEs, AI-assistant hosts and portwise itself
  are refused unless you pass `--allow-protected` (the MCP server always refuses).
- **No surprises:** a PID-reuse guard, `--dry-run` for every plan, and a check that the port is
  really free afterwards.

## Troubleshooting

- **`command not found: portwise`** right after installing: open a new terminal, or run
  `source ~/.config/portwise/env.sh`.
- **Upgrading from v0.1.0:** remove the old copy with `rm ~/.cargo/bin/portwise`.
  `command -v portwise` shows which one runs.
- **`Permission denied` on a shell rc file:** an old `sudo` left it owned by root. Run
  `sudo chown "$USER" ~/.bash_profile` and install again. Never run the installer with `sudo`.
- **Other users' ports are hidden:** run with `sudo` (an elevated terminal on Windows).
- **Uninstall:** `rm ~/.local/bin/portwise`, remove the `env.sh` line the installer added to your
  shell rc files, and delete the app. Settings and history live in `~/.config/portwise` (macOS:
  `~/Library/Application Support/portwise`, Windows: `%APPDATA%\portwise`).

## Learn more

- [CLI reference](docs/cli.md): every command and flag.
- [Architecture](docs/architecture.md): how the core, CLI, TUI, desktop app and MCP server fit together.
- [Changelog](CHANGELOG.md) and [security policy](SECURITY.md).

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, checks and
conventions, including building the desktop app.

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Unless
you state otherwise, any contribution you submit is dual-licensed the same way. The desktop app
bundles Inter and JetBrains Mono under the SIL Open Font License 1.1.
