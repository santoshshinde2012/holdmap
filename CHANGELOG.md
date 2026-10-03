# Changelog

All notable changes to portwise are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Service topology** (`portwise-core::topology`): a graph of services built from local
  ESTABLISHED connections, with listeners folded into service roots and remote peers collapsed per
  host. Clusters come from Compose projects, Kubernetes namespaces, supervisors (pm2, turbo,
  concurrently, nx), workspace roots and git repositories. Tunnels and port-forwards are detected
  (`tunnel.rs`: kubectl, `ssh -L`, cloudflared, ngrok). Exporters for tree, JSON, DOT and Mermaid.
- **Dependency-ordered cluster stop** (`stop_order`, `Engine::plan_cluster`, `Target::Cluster`):
  dependents stop first, and outside dependents become warnings.
- **CLI commands**: `graph` (alias `mesh`; `--json`, `--dot`, `--mermaid`, `--cluster`,
  `--no-external`), `stop --cluster NAME` and `stop cluster:NAME`, `watch` (`--json` for NDJSON),
  `pin`, `unpin`, `pins`, `history`, `restart`, `open`, and `ssh HOST [list|graph]` for agentless,
  read-only remote inspection.
- `portwise man --out-dir DIR` writes `portwise.1` and one man page per command.
- CPU usage per process and per service tree.
- **TUI**: a graph tab (`Tab`/`v`, the selection follows), `C` to stop a cluster, `h` to toggle
  external hosts, and stops recorded in history.
- **MCP**: `get_topology` and `plan_cluster_stop` (always a dry run).
- **Desktop app**:
  - A graph view (Svelte Flow and dagre): cluster hulls, directed edges with traffic dots, hover
    highlighting, shared selection, a minimap, layered and force layouts, both themes and reduced
    motion. `g` switches between list and graph, and `s` stops the selected service's cluster.
  - Pins (`p`, and `⇧P` for a pin with a label), a history panel with restart (`h`), a *Cluster*
    sort and grouping, and *Connections* in the details pane.
  - Settings (`⌘,`): launch at login, global-shortcut presets (re-registered live), theme, list
    density, notifications, scan interval (1–60 s, which also drives the background watcher) and
    history size. Changes save immediately.
  - A remote-host dialog that scans another machine over SSH. Hosts are validated in the UI and
    again in the core, so a value starting with `-` can't inject an ssh option. Recent hosts are
    remembered.
  - Notifications for new and conflicting listeners, launch at login (starts hidden in the tray)
    and a global shortcut (`⌘⌥P` / `Ctrl+Alt+P` by default).
  - A UI kit (`apps/desktop/src/components/ui`): TextField, NumberInput, Select, Switch, Checkbox,
    SegmentedControl, FilterChip, Tabs, Button, IconButton, Kbd, Dialog, ScrollArea, CopyValue,
    Callout, Splitter, SettingRow, SettingsGroup and RichText, a shared tooltip action, and a
    `#ui-gallery` reference page.
  - A ⌘K command palette, framework tiles, a process-chain view ("What Stop will signal"), a stop
    flow with progress and success states, toasts with actions, skeleton rows, a free-port answer,
    first-run hints and a richer tray menu.
  - Compact density (36 px rows), a CPU sparkline and memory column, collapsible sections (click
    the header, or `←`/`→`), and Home/End and PageUp/PageDown in the list.
  - Bundled fonts: Inter Variable for the UI and JetBrains Mono Variable for code, PIDs and paths,
    loaded locally with `font-display: swap`.
- **Documentation**: a rewritten README, a [user guide](docs/user-guide.md), a generated
  [CLI reference](docs/cli.md), an [MCP guide](docs/mcp.md), [development](docs/development.md)
  and [release](docs/releasing.md) guides, [architecture](docs/architecture.md) diagrams, a
  [research audit](docs/audit.md), `SECURITY.md`, `CODE_OF_CONDUCT.md`, and GitHub issue and
  pull request templates.
- `scripts/gen-docs.sh` generates the man pages, shell completions and `docs/cli.md`;
  `scripts/capture-desktop-screenshots.mjs` and `scripts/capture-terminal-screenshot.sh` recreate
  the screenshots.
- **Checks**:
  - `cargo test` verifies that `docs/cli.md` matches the clap definitions, that the README
    mentions every command and only real flags, that Markdown links, images and anchors
    resolve, that no screenshot is orphaned, and that file names follow the conventions in
    `CONTRIBUTING.md`.
  - CI builds the Rust docs with `-D warnings`. `portwise-core` and `portwise-mcp` document every
    public item.
  - A typography lint (`apps/desktop/src/typography.test.ts`) and a WCAG AA contrast lint
    (`apps/desktop/src/contrast.test.ts`).
  - proptest suites (DAG stop order, deterministic ordering, parsers that never panic),
    criterion benchmarks (`cargo bench -p portwise-core`), topology end-to-end tests with real
    connected services, and vitest and Testing Library tests for the desktop UI.

### Changed

- **One protection rule for every surface.** portwise's own process tree (including the processes
  it started) is hard-protected. Interactive shells, terminals, IDEs, IDE remote servers and
  AI-agent hosts, and their ancestors, are soft-protected. Plans say whether a block is
  `overridable`, and the desktop app only offers "stop it anyway" when it is, behind an explicit
  "I understand" checkbox.
- **Core structure**: `SocketProvider`, `ProcessProvider` and `ContainerProvider` behind a
  `Scanner`; one `StopStrategy` per owner kind in a `StrategyRegistry`; a `ProtectionPolicy` trait;
  `project/` split into `ProjectDetector`, `ManifestRegistry` and `WorkspaceMarker`s.
- **Desktop backend** split into `commands`, `state`, `tray`, `watch` and `shortcuts`. The
  background re-scan runs at 2.5× the scan interval (at least every 10 s) while the window is hidden.
- **Desktop details pane**: a header with the port, status and quick actions; Overview,
  Connections, Process, Network and Commands tabs; a sticky footer with Open, Force kill and Stop;
  values that truncate with a tooltip and copy; resizable in wide windows and a focus-trapped sheet
  in narrow ones.
- **Desktop port list**: single-line rows on a fixed grid, a leading status dot, at most two badges
  per row by priority (the rest fold into "+N"), actions that overlay the trailing columns instead
  of reserving space, sticky collapsible section headers, clearer hover, selection and focus
  states, and columns that drop in order as the pane narrows.
- **Typography**: one type scale (11–32 px, base 13 px) with nine semantic roles, weights limited
  to 400/500/600, and about 180 ad-hoc sizes and weights replaced with tokens. The TUI and CLI map
  the same roles onto bold and colour.
- Toolbar with segmented controls, filter chips with counts and a sort select. Search has a clear
  button and a `/` hint.
- Accessibility: AA contrast for placeholders, hints and solid buttons in both themes, ARIA
  listbox semantics in the list, no native `title` tooltips, and focus rings only for keyboard users.
- TUI: section headings in the details pane, an accent port column and grouped, context-aware
  footer hints. CLI: ports are bold cyan.
- `scripts/demo-servers.sh start` is idempotent and additive; `stop` stops everything any run started.
- **File names follow one convention per ecosystem** (see `CONTRIBUTING.md`): docs under `docs/`
  are kebab-case (`docs/architecture.md`, `docs/audit.md`), screenshots are
  `<surface>-<view>-<theme>.png`, the bundled fonts are kebab-case, and the desktop unit tests sit
  next to the module they test (`roving.test.ts`, `validate.test.ts`, `palette.test.ts`…).

### Fixed

- The desktop graph no longer hits Svelte's "maximum update depth exceeded" error when a selection
  centres the viewport with reduced motion on.
- `portwise unpin` no longer accepts a `--label` option it ignored.
- Every CLI argument and `--json` flag now has help text, so `--help` and the man pages are complete.
- The status dot in list rows was squeezed into a bar, and long graph edge labels now truncate.
- The browser preview of the desktop app records stops, so its history panel works.
- Documentation: corrected the VS Code MCP configuration (it uses `servers` and `type: "stdio"`),
  the desktop bundle size, the hotkey presets in the `Config` docs, the stop-strategy order, and
  several stale file paths.

### Removed

- About 100 outdated screenshots (the `before/` set and intermediate `ui-*`, `type-*` and
  `list-*` captures) that no document used. They remain in the git history.

## [0.1.0] - 2026-10-03

### Added

- `portwise-core`:
  - Socket → PID enumeration for Linux (`/proc`), macOS (libproc) and Windows (IP Helper).
  - Process trees, project, framework and git-branch detection, and Docker, Podman, OrbStack and
    Colima container mapping.
  - Plain-English `explain`, previewable `ActionPlan`s, and graceful tree stop with SIGKILL
    escalation, a PID-reuse guard (pidfd on Linux) and port-free verification.
  - A protected-process policy, and supervisor awareness (systemd, pm2, Homebrew services).
  - OS-feature explanations (AirPlay Receiver, HTTP.sys, Windows excluded port ranges).
- `portwise` CLI: `list`, `inspect`, `explain`, `stop`, `kill`, `free-port`, `wait`, `run -p`,
  `completions` and `man`, with `--json` output and documented exit codes.
- `portwise tui`: a keyboard-first ratatui interface with search, filters, sorting, a details
  pane, explain and confirmed stop.
- `portwise mcp`: an MCP stdio server with the `list_ports`, `explain_port`, `find_free_port`,
  `wait_for_port` and `stop_port` tools.
- Desktop app (Tauri v2 and Svelte 5): a grouped live list, search and filters, an explain pane,
  confirmed stop with live progress, light and dark themes, keyboard shortcuts and a tray menu.

[Unreleased]: https://github.com/santoshshinde/portwise/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/santoshshinde/portwise/releases/tag/v0.1.0
