# Changelog

All notable changes to portwise are documented here. The project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). From 0.1.1 on, entries are generated
by [release-please](https://github.com/googleapis/release-please) from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/); see
[Releasing](CONTRIBUTING.md#releasing).

## [0.1.0] - Unreleased

The first public release.

### Added

- **Core** (`portwise-core`): socket → PID enumeration for Linux (`/proc`), macOS (libproc) and
  Windows (IP Helper); process trees; project, framework and git-branch detection; Docker,
  Podman, OrbStack and Colima container mapping; plain-English `explain`; previewable stop plans
  with graceful tree stop, SIGKILL escalation, a PID-reuse guard and port-free verification;
  supervisor awareness (systemd, pm2, Homebrew services); OS-feature explanations (AirPlay
  Receiver, HTTP.sys, excluded port ranges); one protection rule for every surface (portwise's own
  tree, shells, terminals, IDEs and AI-agent hosts).
- **Service graph**: who talks to whom over local connections, grouped into clusters (Compose,
  Kubernetes namespace, pm2/turbo/nx, workspace, git repo), with tunnels and port-forwards, and a
  dependency-ordered cluster stop. Exported as a tree, JSON, DOT or Mermaid.
- **Project stacks**: a `.portwise.toml` with `portwise up`, `down`, `status` and `init`. Services
  start in dependency order and are waited on; a different program on a project port is reported
  as a conflict (`--replace` stops it after confirmation); `protect` ports are never stopped
  without `--allow-protected`.
- **HTTP probe**: `list --http`, `inspect`, `status` and the desktop details pane show each web
  port's status code and page title.
- **Stop all dev servers**: `stop --all-dev`, the tray menu and the desktop command palette stop
  every dev server of yours in one confirmed plan.
- **Shell hook**: `portwise init zsh|bash|fish|pwsh` explains a port-in-use failure after a
  dev-server command and prints the command that frees it.
- **CLI**: `list`, `inspect`, `explain`, `stop`, `kill`, `free-port`, `wait`, `run -p`, `graph`,
  `watch`, `pin`/`unpin`/`pins`, `history`, `restart`, `open`, `ssh HOST` (read-only remote
  inspection), `completions` and `man`, with `--json` everywhere and documented exit codes.
- **TUI**: search, filters, sorting, a details pane, a graph tab, confirmed stop and cluster stop.
- **MCP server**: `list_ports`, `explain_port`, `find_free_port`, `wait_for_port`, `stop_port`,
  `get_topology` and `plan_cluster_stop`.
- **Desktop and tray app** (Tauri v2, Svelte 5): a grouped live list, details pane, stop flow with
  live progress, service graph, pins, history with restart, remote hosts over SSH, a command
  palette, settings, notifications for new and conflicting listeners, launch at login and a
  global shortcut.

### Performance

- A warm scan takes about 19 ms on Linux (was 60 ms): process refresh no longer walks every
  thread's `/proc/<pid>/task` entry.

[0.1.0]: https://github.com/santoshshinde2012/portwise/commits/main
