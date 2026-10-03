# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).

## Unreleased: UI/UX polish

- Desktop: added a design system (spacing, radius, type and motion tokens) with AA-checked light and dark themes, a ⌘K command palette,
  framework tiles, a hero port number, status badges, a process-chain view in the details pane, a stop flow with progress and success states,
  toasts with actions, skeleton rows, an animated empty state, a free-port answer, first-run hints, a narrow-window drawer, reduced-motion support and a richer tray menu.
- New `free_port` Tauri command.
- TUI: added section headings in the details pane, an accent port column, and grouped context-aware footer hints.
- CLI: the port column is now bold cyan.

## [Unreleased]

## [0.1.0] - 2026-10-03

### Added
- `portwise-core`:
  - Socket → PID enumeration for Linux (`/proc`), macOS (libproc) and Windows (iphlpapi).
  - Process tree, project/framework/git-branch detection, and Docker/Podman/OrbStack/Colima container mapping.
  - Plain-English `explain`, previewable `ActionPlan`s, and graceful tree stop with SIGKILL escalation, a PID-reuse guard (pidfd on Linux) and port-free verification.
  - Protected-process policy, and supervisor awareness (systemd, pm2, Homebrew services).
  - OS-feature explanations (AirPlay, HTTP.sys, Windows excluded port ranges).
- `portwise` CLI:
  - Commands: `list`, `inspect`, `explain`, `stop`, `kill`, `free-port`, `wait`, `run -p`, `completions` and `man`.
  - `--json` output and documented exit codes.
- `portwise tui`: keyboard-first ratatui interface with search, filters, sorting, a details pane, explain and confirmed stop.
- `portwise mcp`: MCP stdio server with the `list_ports`, `explain_port`, `find_free_port`, `wait_for_port` and `stop_port` tools.
- Desktop app (Tauri v2 + Svelte 5):
  - Grouped live list, search and filters, explain pane, and confirmed stop with live progress.
  - Light/dark themes, keyboard shortcuts and tray menu.
