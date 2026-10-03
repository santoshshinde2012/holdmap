# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).

## Unreleased: typography

### Changed
- **Bundled fonts**: Inter Variable (UI) and JetBrains Mono Variable (code, PIDs, paths) now
  ship inside the app, loaded with `font-display: swap` and no CDN. Before, the app named Inter
  but never shipped it, so macOS fell back to a mix of SF Pro and SF Mono.
- **One type scale** (11/12/13/14/16/20/24/32, base 13 px) with nine semantic roles (display,
  title, heading, body, body-sm, caption, label, mono, mono-sm). Weights are limited to
  400/500/600, and line heights and tracking are defined per role. About 180 ad-hoc sizes and
  weights (10px, 10.5px, 11.5px, 12.5px, 550, 650, 700 …) were replaced with tokens.
- **Calmer hierarchy**: port numbers are set in Inter with tabular figures, and labels, counts,
  palette items and toasts drop from semibold to medium or regular. Stat tiles, empty states
  and dialogs use consistent title and heading sizes.
- **Fixes**: the status dot in list rows was squeezed into a bar, and long graph edge labels
  now truncate.
- **Terminal**: the TUI uses semantic styles (`tui/theme.rs`), and graph ports match the list
  table (accent and bold).

### Added
- A typography lint (`lib/typography.test.ts`) that fails on raw font values, and a
  type-scale specimen on the `#ui-gallery` page.

## Unreleased: desktop UI/UX redesign

### Added
- **UI kit** (`apps/desktop/src/components/ui`): TextField, NumberInput, Select, Switch,
  Checkbox, SegmentedControl, FilterChip, Tabs, Button, IconButton, Kbd, Tooltip, Dialog,
  ScrollArea, CopyValue, Callout and Splitter. They share one set of heights, radii and focus
  rings, plus a `#ui-gallery` reference page.
- **Settings** (⌘/Ctrl + ,): launch at login, global-shortcut presets (re-registered live),
  theme, notifications, scan interval (1–60 s, which also drives the background watcher) and
  history size. Changes save automatically.
- **Pin dialog** (⇧P): pin any port with a label, with validation and an "in use by" hint.
- **Remote host dialog**: scan another machine over SSH. The host is validated in the UI and
  again in the core, so a value starting with `-` can't inject an ssh option. Recent hosts are
  remembered.

### Changed
- **Details pane**:
  - a header with the port, status and quick actions
  - Overview / Connections / Process / Network / Commands tabs
  - a sticky footer with Open, Force kill and Stop
  - scroll shadows, and long values that truncate with a tooltip and copy
  - resizable in wide mode (drag, arrow keys, double-click to reset)
  - a focus-trapped sheet in narrow windows
- Stop-anyway asks for an explicit "I understand" checkbox. Confirm, palette, history and
  shortcuts dialogs are rebuilt on the kit.
- Toolbar uses segmented controls, filter chips with counts and a custom sort select. Search
  gains a clear button and a `/` hint.
- Accessibility: AA contrast for placeholders, hints and solid buttons in both themes; no
  native `title` tooltips; visible focus only for keyboard users.

## Unreleased: topology, mesh and research audit

### Added
- **Topology** (`portwise-core::topology`): a service graph built from local ESTABLISHED connections,
  with listeners folded into service roots and remote peers collapsed per host. It has clusters
  (Compose → Kubernetes → supervisor → workspace → git), tunnel and port-forward detection
  (`tunnel.rs`: kubectl, `ssh -L`, cloudflared, ngrok), dependency-ordered stop
  (`stop_order`, `Engine::plan_cluster`, `Target::Cluster`), and exporters (tree, JSON, DOT, Mermaid).
- CLI:
  - `graph` / `mesh` (`--json`, `--dot`, `--mermaid`, `--cluster`, `--no-external`)
  - `stop --cluster NAME` and `stop cluster:NAME`
  - `watch [--json]`
  - `pin`, `unpin`, `pins`
  - `history`, `restart`
  - `open`
  - `ssh HOST [list|graph]` (agentless remote)
- CPU % per process.
- TUI:
  - graph tab (`Tab`/`v`, selection follows)
  - `C` stops a cluster
  - `h` toggles external hosts
  - stops are recorded in history
- Desktop:
  - graph view (Svelte Flow + dagre: cluster hulls, directed edges with traffic dots, hover
    highlighting, shared selection, minimap, layered/force layouts, both themes, reduced motion)
  - list ⇄ graph (`g`), *Cluster* sort and grouping
  - pinned section (`p`), history panel with restart (`h`), cluster stop (`s`) with an ordered plan
  - *Connections* in the details pane
  - notifications for new and conflicting listeners, launch at login (`--hidden`), global hotkey `Ctrl+Alt+P` / `⌘⌥P`
- MCP: `get_topology`, `plan_cluster_stop`.
- Tests:
  - proptest (DAG stop order, deterministic ordering, parsers never panic)
  - criterion benches (`cargo bench -p portwise-core`)
  - topology e2e with real connected services
  - vitest for graph layout/ordering
- Docs:
  - `docs/AUDIT.md`: a feature matrix of every research point
  - `docs/ARCHITECTURE.md`: mermaid diagrams and the graph library rationale
- New screenshots.

### Changed
- SOLID refactor of the core:
  - `SocketProvider`/`ProcessProvider`/`ContainerProvider` behind a `Scanner`
  - `engine/` split into one `StopStrategy` per owner kind in a `StrategyRegistry`
  - `ProtectionPolicy` trait
  - `project/` split into `ProjectDetector`, `ManifestRegistry` and `WorkspaceMarker`s
- Desktop backend split into `commands`, `state`, `tray`, `watch` and `shortcuts`. The background re-scan
  slows to every 10 s while the window is hidden.

## Unreleased: shared protection rule

- Core: one protection rule shared by every frontend. Portwise's own process tree (including the processes it started) is
  hard-protected. Interactive shells, terminals, IDEs, IDE remote servers and AI-agent hosts, and their ancestors, are
  soft-protected. Plans now report whether a block is `overridable`, and the desktop app only offers "stop anyway" when it is.
- `scripts/demo-servers.sh start` is now idempotent and additive (it records names and pids and never truncates), and
  `stop` stops everything any run started. It also no longer keeps the caller's stdout open.

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
