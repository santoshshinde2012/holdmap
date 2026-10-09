# Changelog

All notable changes to holdmap are documented here. The project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Once release-please has its token, entries are generated
by [release-please](https://github.com/googleapis/release-please) from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/); see
[Releasing](CONTRIBUTING.md#releasing).

## [Unreleased]

### Changed

- **Renamed from portwise to holdmap.** The product maps agents, tools and apps — not only ports.
  Binary, crates, config dir (`~/.config/holdmap`), project file (`.holdmap.toml`), env vars
  (`HOLDMAP_*`), MCP server name, desktop bundle id (`dev.holdmap.app`) and site base path all
  use the new name. Legacy `$PORTWISE_HOME` / `$PORTWISE_*` env vars, a leftover `portwise`
  config directory (migrated once), and `.portwise.toml` project files are still accepted.

### Added

- **Control from the Agents surface.** Reveal a folder, open it in your editor, or stop the
  unprotected ports an agent started (desktop cards, `holdmap agents --stop-ports`, MCP
  `stop_agent_ports`). Paths are verified against the current agents report before opening.
- **Developer tools in the catalog.** Docker Desktop, OrbStack and Podman Desktop appear beside
  AI coding agents, with the same folders / ports / access footprint.
- **TUI Agents tab.** Cycle Ports → Graph → Agents with Tab; browse each agent's footprint.
- **Site and docs** reframe the problem around agents, tools and apps — not only ports. Marketing
  pages, SEO copy, Interfaces, Features (agents screenshot), Footer and a new
  [Agents guide](https://santoshshinde2012.github.io/holdmap/docs/agents/) cover control, richer
  skim and developer tools.

- Agents empty states, onboarding, shortcuts and MCP copy talk about tools and apps as well as
  coding agents. Desktop hides port filters while the Agents tab is open and prefetches the badge
  count.
- **Richer agent skim surfaces:** memory, CPU, process count, stoppable count and parent on cards /
  graph nodes / CLI / TUI / MCP text; child processes grouped as “tools & apps it started”; folder
  source labels (working dir / child cwd / recent); TUI `x` stops an agent’s ports; demo includes
  Docker Desktop.
- Product copy (CLI `--help`, desktop store metadata, README architecture, site CLI demo) aligns
  with agents and tools; removed a duplicate desktop `icon.svg`.

## [0.2.0] - 2026-10-09

### Added

- **AI coding agents, end to end.** holdmap now finds the coding agents running on your machine
  (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider, Cline and others) and shows,
  for each one, the folders it works in, the ports it serves, the services and hosts it talks to,
  and its access: account, sandbox, approval flags, network listeners and macOS-protected
  folders. Each fact is marked seen, inferred or unknown. Chats, settings and tokens are never
  read.
  - **Desktop: Agents view** (`⇧A`): a card per agent next to a map of their shared footprint.
    Click a port to open its details.
  - **CLI: `holdmap agents [AGENT] [-w] [--json]`.**
  - **MCP: `list_agents`**, read-only.
- **Website** at <https://santoshshinde2012.github.io/holdmap/> with a live, in-browser demo of the
  desktop app, including the Agents view with sample agents, install steps for every platform and
  a short guide. It is built from `site/` and deployed by the Pages workflow.

## [0.1.9] - 2026-10-08

### Fixed

- **Desktop: Force layout no longer looks empty** when services have no links between them. Unlinked
  nodes were pushed thousands of pixels apart; a gentle pull now keeps them in view.
- **Release notes no longer show a `brew install` line** that didn't work yet. Homebrew returns
  once the tap is published.

## [0.1.8] - 2026-10-08

### Fixed

- **Desktop: Graph view no longer rebuilds the topology in a loop.** It re-fetched the graph as soon
  as the previous one arrived, costing CPU while Graph was open.

### Changed

- README and release screenshots show the current UI (light and dark).

## [0.1.7] - 2026-10-05

### Fixed

- **macOS desktop release checksums:** installer re-upload no longer uses `mapfile` (bash 3.2
  on macOS runners), so `holdmap-desktop-macos-*.sha256` can be published after the DMG.
- **Windows desktop release:** NSIS `setup.exe` and `holdmap-desktop-windows-x64.sha256` no longer drop
  when the upload flaked after MSI. Collect runs whenever Tauri produced output, and installer/
  checksum uploads retry with longer timeouts.

### Performance

- Watch path borrows the snapshot (one fewer clone per scan).
- Desktop skips Live pushes while minimized and uses the slow cadence when minimized/occluded.
- Graph / Svelte Flow loads only when you open Graph (~188 KB JS deferred from startup).
- Live list updates coalesce onto `requestAnimationFrame`.

### Measured (before → after where applicable)

- CLI `list` / scan already ~40 ms / ~32 ms on Mac (unchanged; no scan rewrite).
- Vite production: GraphView split into its own chunk (`GraphView-*.js` ~188 KB / ~60 KB gzip);
  initial `index-*.js` no longer embeds the graph.
- Slimming IPC by dropping cmdline/exe skipped (would empty Process details without a larger API).
- List virtualization and font subsetting skipped (short lists; font risk).

## [0.1.6] - 2026-10-05

### Fixed

- **Desktop: blank white screen after long runs.** An uncaught error could leave the WebView
  empty (only the native title bar). The app now remounts and tells you why. Explain/HTTP/
  details caches and sparkline samples have hard size caps, history is dropped while the
  window is hidden, and the service graph is only built in Graph view.

### Added

- **Accurate memory.** macOS uses Activity Monitor's `phys_footprint`, Linux `VmRSS`, Windows
  Working set. Each port shows the app total (process + helpers) with a helper count in the
  list, details, graph, CLI, JSON and MCP. Heavy apps get a memory badge; sort-by-memory uses
  the app total; a copy-only quit-helpers command appears when safe.

## [0.1.5] - 2026-10-04

### Fixed

- **Desktop (macOS): the list no longer freezes behind the Documents privacy prompt.** Project
  detection read files in each process's folder during the scan, and macOS holds the first read
  under `~/Documents`, `~/Desktop` or `~/Downloads` until the prompt is answered, so the header
  counted up instead of staying Live. Project lookups now run in the background with a short
  budget and a per-folder cache; project and branch fill in when ready and stay put while they
  refresh. If access is denied or a read hangs, the row just has no project.
- **Desktop (macOS)**: the privacy prompt now says why holdmap reads those folders.

## [0.1.4] - 2026-10-04

### Added

- **Details pane**: who is connected (local apps by name, remote peers, counts), the full
  process tree, start time, CPU and memory trend lines, and the bind address explained in plain
  words with a fix. Loaded lazily for the selected port; nothing flashes while it refreshes.
- **Quick actions**: Restart (a confirmed stop, then the same command in the same folder), open
  the folder in your editor or Finder, copy the URL, a `curl` or the kill command. Force kill moved
  into this menu (`⇧⌫` still works).
- `holdmap inspect` and MCP `explain_port` show the same connections, process tree, start time,
  bind risk and HTTP status (`details` and `http` in the JSON).

### Fixed

- **Desktop**: a service restarted from the app (or its history) was listed as "Protected" and
  couldn't be stopped again, because it counted as part of holdmap's own process tree.

### Security

- Secrets in command lines are hidden everywhere: CLI, TUI, JSON, MCP, the app and history views.
  The history file keeps the real command (so restart works) and is now `0600`; logs too.
- Control characters from processes, manifests and local web pages are stripped before printing,
  so they can't inject terminal escape sequences.
- Desktop: the webview may only listen for events and drag the window, under a stricter CSP
  (`script-src 'self'`; `object-src`, `base-uri`, `frame-ancestors` and `form-action` `'none'`).
  Restart names a history entry and the backend reads the command from its own file; the opener
  plugin is gone (URLs and folders are looked up by port from the scan).
- URLs are opened without `cmd.exe` on Windows and only when they're plain `http(s)`.
- A `.holdmap.toml` owned by another user or writable by anyone is refused.
- MCP: `stop_port` says to confirm with the user first; protected processes stay refused even if
  an agent passes extra arguments (tested).

## [0.1.3] - 2026-10-04

### Fixed

- **Desktop: the header no longer drops to "12s ago" on macOS.** The window's own poll timer
  can be throttled or paused by macOS (WebKit, App Nap, covered windows), and the age counted
  from the last snapshot even while a scan was running. The app's watcher now pushes every
  scan to the window, the window catches up as soon as it's focused or uncovered, and "Live"
  holds while a scan is in flight.
- **Desktop**: scans never overlap; the window, the watcher and the tray share one scan per
  interval (previously up to two). The scan no longer waits for the tray menu to update.

### Changed

- **Core**: the user list (a directory-service query on macOS) is cached for a minute instead
  of being re-read on every scan.
- `HOLDMAP_TRACE=scan` prints how long each part of a scan took (sockets, processes,
  containers), for the CLI and the desktop app.

## [0.1.2] - 2026-10-04

### Fixed

- **Desktop: the details pane no longer blinks.** Every scan (every 3 s) threw away the selected
  port's explanation, so the Overview dropped to its loading skeleton, the stop plan vanished and
  the tab counts and footer reset until it reloaded. Details now stay on screen and refresh in
  the background; the skeleton only shows the first time a port is opened.
- **Desktop**: polls reuse unchanged data, so rows, the graph and sparklines only update what
  changed (about half the idle CPU with 400 ports); the pane keeps its scroll position across
  scans; background scans don't spin the refresh button; the "Live" label no longer flickers to
  "4s ago" between scans; the HTTP status is re-checked every 15 s instead of never; the tray
  menu is only rebuilt when its content changes, so an open menu isn't closed by a scan.

## [0.1.1] - 2026-10-04

### Fixed

- **macOS desktop app "is damaged and can't be opened"**: the app is now ad-hoc signed as a whole
  bundle (v0.1.0 carried only the linker's signature on the binary, which Gatekeeper rejects once
  a download is quarantined). macOS now shows the usual one-time "Open Anyway" prompt; every
  release checks the app inside each .dmg.
- **Installer**: the CLI installs to `~/.local/bin` (was `~/.cargo/bin`), so it works at once
  where that folder is already on `PATH`. Coming from 0.1.0, delete `~/.cargo/bin/holdmap`.
- **CLI**: `stop`/`kill` reject port 0 and ports above 65535; `up` keeps starting services that
  don't depend on one that failed; `unpin` of an unpinned port says so; `run` explains why it
  didn't stop the holder; hidden-listener counts match the rows shown; `ssh` only asks for a
  remote TTY when interactive, so piped and `--json` output stay clean.
- **TUI**: no panic without a terminal (clear message, exit 2); section rules fit the panel
  width; header stats drop whole items on narrow terminals; "1 port", not "1 ports".
- **Core**: sockets with no local port are hidden; full Linux process names past 15 bytes;
  retitled processes no longer show their environment as the command line; multi-line `sh -c`
  scripts aren't dev-server launchers; container runtimes' own ports aren't dev servers; command
  output can't deadlock; correct plurals everywhere.
- **MCP**: `stop_port` is recorded in the history; `list_ports` counts hidden listeners properly.
- **Release**: bare file names in the Windows desktop checksum file; the desktop build checks out
  the release tag through dist's `workflow_call` (CodeQL alert fixed).

### Changed

- **Desktop design pass**: an "At a glance" summary in the empty details pane; details facts in
  one divided card; the command palette keeps its sections while searching, with quieter
  destructive icons; the stop confirmation names its target ("Stop :3000"); WCAG AA contrast for
  faint text on selected rows; keyboard focus stays in the list after the welcome banner; a
  distinct "match system" theme icon; the Graph badge reads "6 links" to screen readers.
- **Desktop graph**: readable faded nodes and motion-token transitions; edge labels fit between
  their nodes; the graph fits below its toolbar and legend; the minimap only appears when part of
  the graph is off-screen, translucent until hovered.

## [0.1.0] - 2026-10-03

The first public release.

### Added

- **Core** (`holdmap-core`): socket → PID enumeration for Linux (`/proc`), macOS (libproc) and
  Windows (IP Helper); process trees; project, framework and git-branch detection; Docker,
  Podman, OrbStack and Colima container mapping; plain-English `explain`; previewable stop plans
  with graceful tree stop, SIGKILL escalation, a PID-reuse guard and port-free verification;
  supervisor awareness (systemd, pm2, Homebrew services); OS-feature explanations (AirPlay
  Receiver, HTTP.sys, excluded port ranges); one protection rule for every surface (holdmap's own
  tree, shells, terminals, IDEs and AI-agent hosts).
- **Service graph**: who talks to whom over local connections, grouped into clusters (Compose,
  Kubernetes namespace, pm2/turbo/nx, workspace, git repo), with tunnels and port-forwards, and a
  dependency-ordered cluster stop. Exported as a tree, JSON, DOT or Mermaid.
- **Project stacks**: a `.holdmap.toml` with `holdmap up`, `down`, `status` and `init`. Services
  start in dependency order and are waited on; a different program on a project port is reported
  as a conflict (`--replace` stops it after confirmation); `protect` ports are never stopped
  without `--allow-protected`.
- **HTTP probe**: `list --http`, `inspect`, `status` and the desktop details pane show each web
  port's status code and page title.
- **Stop all dev servers**: `stop --all-dev`, the tray menu and the desktop command palette stop
  every dev server of yours in one confirmed plan.
- **Shell hook**: `holdmap init zsh|bash|fish|pwsh` explains a port-in-use failure after a
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

[0.2.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.9...v0.2.0
[0.1.9]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/santoshshinde2012/holdmap/releases/tag/v0.1.0
