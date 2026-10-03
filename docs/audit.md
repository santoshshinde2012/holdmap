# portwise — research audit (feature matrix)

Every point in the research documents (`port-research/01`–`06`) checked against the code on
**3 Oct 2026**, re-audited after the documentation pass (commit after `16867a6`). Status: ✅ done · 🟡 partial · ❌ missing · ⏭ skipped on purpose · — not applicable.
"Tests" names where the behaviour is covered: `core` = unit tests in that module,
`e2e` = `crates/portwise-core/tests/e2e_linux.rs` (real sockets and processes), `cli` =
`crates/portwise-cli/tests/cli.rs` (assert_cmd), `mcp` = `crates/portwise-mcp`, `prop` =
proptest (`topology/proptests.rs`), `bench` = criterion (`crates/portwise-core/benches/scan.rs`),
`vitest` = `apps/desktop/src/**/*.test.ts` (logic, components in jsdom, typography and contrast
lints), `desk` = `apps/desktop/src-tauri` unit tests, `docs` = the docs and convention checks
(`crates/portwise-cli/src/cli_docs.rs`, `crates/portwise-cli/tests/repo_conventions.rs`).

## Summary

| | ✅ done | 🟡 partial | ❌ missing | ⏭ skipped | total |
|---|---|---|---|---|---|
| 01 problem statement (P1–P14) | 10 | 4 | 0 | 0 | 14 |
| 02 table stakes + gaps | 13 | 2 | 0 | 0 | 15 |
| 03 core + differentiators 1–22 + UX | 19 | 8 | 3 | 3 | 33 |
| 04 platform internals | 18 | 6 | 2 | 2 | 28 |
| 05 tech stack | 3 | 1 | 0 | 0 | 4 |
| 06 design decisions + testing strategy | 10 | 5 | 1 | 0 | 16 |
| topology / mesh and SOLID | 14 | 0 | 0 | 0 | 14 |
| product quality: desktop UX, typography, docs | 10 | 0 | 0 | 0 | 10 |
| **total** | **97** | **26** | **6** | **5** | **134** |

**Changed in this re-audit**: graceful Windows stop moved from ❌ to 🟡 (the code already sends
WM_CLOSE through `taskkill` before `TerminateProcess`; the audit had missed it); GUI testing now
includes component tests; benchmark numbers re-measured; and a new section covers the desktop
UX, typography and documentation work, all ✅.

**Closed in earlier rounds**: pins, history of stopped ports and restart, notifications for new
and conflicting listeners, launch at login, the global hotkey, `portwise open`, `restart`, `watch`
(NDJSON), CPU per process and per service tree, agentless remote over SSH, Kubernetes
port-forward / `ssh -L` / cloudflared / ngrok detection, compose/workspace/supervisor/git
clustering, the topology graph on every surface, dependency-ordered cluster stop, proptest and
criterion.

**Still open, with reasons** (also the [roadmap](../README.md#roadmap)): privileged helper (needs per-OS signing + an installer; unsafe to
ship unsigned) · eBPF/netlink events (polling meets the budget; eBPF needs root) · closing a
single TCP connection (low value, needs `CONFIG_INET_DIAG_DESTROY` / admin) · CTRL_BREAK for Windows console
apps and Windows-service stop (can't be verified without a Windows machine; only
cross-compiled here) · `.portwise.toml` project config, shell EADDRINUSE hook, IDE/Raycast
extensions (separate deliverables; the JSON/MCP surfaces they'd use exist) · JSON Schema and
`insta` snapshots (assertions are explicit string/JSON checks today) · code signing/notarisation
(needs certificates) · live Docker (no daemon on the build box: container logic is covered by
recorded Engine-API fixtures).

## 01 — Problem statement

| # | Research item | Status | Where in code | Tests |
|---|---|---|---|---|
| P1 | Who owns this port? | ✅ | `scan.rs` (`Scanner`), `sys/{linux,macos,windows}.rs`, CLI `list`/`inspect`/`explain` | e2e, cli |
| P2 | Orphaned dev-server trees | ✅ | `engine/strategies/process_tree.rs`, `engine/context.rs` (`safe_descendants`) | e2e (tree stop), core |
| P3 | Permission blind spots | ✅ | `Snapshot.hidden_sockets`, `NodeKind::Hidden`, `sys/netstat_backend.rs`; UI "N hidden — elevate" | core, cli |
| P4 | docker-proxy holds the port | ✅ | `scan::is_container_forwarder`, `strategies/container.rs`, `docker.rs` (Engine API) | core (API fixtures) |
| P5 | Supervisors respawn | 🟡 | pm2 (`strategies/pm2.rs`), systemd incl. `.socket` (`systemd.rs`), brew (`brew.rs`), launchd *warning*; no `launchctl bootout` / restart-policy handling | core |
| P6 | OS services on dev ports (AirPlay, HTTP.sys) | ✅ | `strategies/os_service.rs` | core |
| P7 | Windows excluded port ranges | ✅ | `windiag.rs` (netsh parser) → explain | core (netsh fixture) |
| P8 | WSL relay | ✅ | `strategies/os_service.rs` (wslrelay) | core |
| P9 | IPv4/IPv6 / bind confusion | ✅ | v4+v6 rows merged (`scan.rs` families), `Exposure`, `events::PortEvent::Conflict` for two owners on one port | core, e2e |
| P10 | TCP/UDP, LISTEN vs ESTABLISHED vs TIME_WAIT | ✅ | `ScanOptions.all_states`, TIME_WAIT owner in `engine` | core, e2e |
| P11 | Wrong-process kills / PID reuse | ✅ | `exec.rs` (start-token check, Hard re-check), pidfd in `sys/linux.rs` | core, e2e |
| P12 | Windows has no SIGTERM | 🟡 | `sys/windows.rs`: `taskkill` without `/F` (WM_CLOSE) first, then `TerminateProcess`; no CTRL_BREAK for console apps | cross-check only |
| P13 | Silent port drift (5173 → 5174 …) | 🟡 | `watch`/notifications show new listeners and conflicts; no explicit "drifted from 5173" hint | core (events) |
| P14 | Supply chain of `npx kill-port` | 🟡 | single static Rust binary, cargo-dist (`dist-workspace.toml`), no npm runtime deps; unsigned so far | — |

## 02 — Existing tools: table stakes and gaps

| Item | Status | Where | Tests |
|---|---|---|---|
| List listeners with PID/name, search, one-command kill | ✅ | `list`, `stop`, TUI, desktop | cli, e2e |
| SIGTERM → SIGKILL escalation | ✅ | `exec.rs` | e2e (SIGTERM-ignoring child) |
| Docker awareness | ✅ | `docker.rs`, container strategy | core |
| Project/cwd labels | ✅ | `project/` (manifest registry, workspace markers, git) | core |
| Tray / menu bar | ✅ | `apps/desktop/src-tauri/src/tray.rs` (macOS/Linux/Windows) | manual |
| Favourites + notifications | ✅ | `store.rs` pins, `events.rs`, desktop `watch.rs` (OS notifications) | core, desk, cli |
| Gap: one core, every surface, every OS | ✅ | core + CLI + TUI + desktop + MCP; macOS/Windows cross-compiled (`scripts/check-cross.sh`) | all |
| Gap: root-cause "why is it busy?" | ✅ | `Engine::explain`, `StrategyRegistry` | core, cli |
| Gap: process-tree kill | ✅ | `process_tree.rs` | e2e |
| Gap: race-free kill | ✅ | pidfd + start token | core |
| Gap: graceful Windows stop | 🟡 | WM_CLOSE via `taskkill`, then `TerminateProcess`; console apps need CTRL_BREAK (see P12) | cross-check only |
| Gap: run / free-port / wait primitives | ✅ | CLI `run`, `free-port`, `wait` | cli |
| Gap: agent-safe interface | ✅ | `portwise-mcp` (read-only tools + policy-gated stop; new `get_topology`, `plan_cluster_stop`) | mcp |
| Gap: Linux desktop tray | ✅ | Tauri tray (appindicator) | manual |
| Gap: trustworthy supply chain | 🟡 | reproducible cargo-dist pipeline; signing/notarisation pending certificates | — |

## 03 — Features

### Core (table stakes)

| Item | Status | Where | Tests |
|---|---|---|---|
| Listener fields (proto, family, bind, state, PID, user, cmdline, exe, uptime, memory, **CPU**) | ✅ | `model.rs` `PortEntry`/`ProcessInfo.cpu_percent`, `provider::SystemProcesses` | core, cli |
| Filter/search language (`:3000-3999`, `proto:udp`, `pid:`) | ✅ | `Filter` | core, vitest |
| Kill by port/PID/name, graceful, verify freed | ✅ | `Target`, `exec.rs` | e2e, cli |
| `--json`, stable exit codes 0/1/2/3/4 | ✅ | `main.rs::exit` | cli |
| Dry-run / preview | ✅ | `ActionPlan`, `--dry-run` | cli |
| All-connections mode | ✅ | `--all` / "All sockets" | cli |

### Differentiators

| # | Item | Status | Where | Tests |
|---|---|---|---|---|
| 1 | Owner resolution & doctor | ✅ | `engine/` + strategies | core, cli |
| 2 | Correct stop action per owner | 🟡 | container, systemd, pm2, brew, process tree, hidden, protected, other-user; missing launchctl/Windows services | core, e2e |
| 3 | Process-tree / group kill | ✅ | `process_tree.rs` | e2e |
| 4 | Safety rails (+ "remember command+cwd to restart") | ✅ | `safety.rs` (`ProtectionPolicy`), `SessionKind`/`HOST_MARKERS`, `history.rs` | core, e2e, cli |
| 5 | Project & framework detection (+ HTTP probe) | 🟡 | `project/`; TCP accept probe only, no HTTP title probe | core |
| 6 | One engine, three surfaces | ✅ | CLI, TUI, desktop, MCP | all |
| 7 | `run` / `free-port` / `wait` | ✅ | CLI | cli |
| 8 | MCP server | ✅ | `portwise-mcp` (7 tools) | mcp |
| 9 | Tray with conflict notifications, launch at login | 🟡 | `watch.rs` notifications, autostart plugin, global shortcut; no "kill all dev servers" button (stop-cluster covers groups) | desk |
| 10 | Windows-first diagnostics | 🟡 | excluded ranges, HTTP.sys, wslrelay explanations; no CTRL_BREAK, no URL-ACL owner | core |
| 11 | Favourites / named & pinned ports | 🟡 | pins with labels (`store.rs`, `pin`/`unpin`/`pins`, ★ in list, desktop star); no `.portwise.toml` | core, cli, vitest |
| 12 | Open in browser / copy URL / reveal / IDE / logs | 🟡 | open + copy everywhere (`portwise open`, TUI `o`/`c`, desktop); restart logs in `logs/`; no reveal-in-Finder/IDE | cli |
| 13 | Exposure warning | ✅ | `Exposure`, badges, details callout | core, vitest |
| 14 | History / event log | 🟡 | JSON-lines stop history with restart (`store.rs`, `history.rs`, desktop History panel); `watch --json` streams events but they aren't persisted | core, cli |
| 15 | Launcher integrations (Raycast/Alfred) | ❌ | — (CLI `--json` is the integration surface) | — |
| 16 | IDE integrations | ❌ | — | — |
| 17 | Remote machines over SSH | ✅ | `remote.rs` (`RemoteRunner`, agentless `ss`+`ps`), CLI `ssh HOST [list|graph]` (forwards to remote portwise when installed) | core (LocalShell e2e), cli |
| 18 | Kubernetes port-forwards & tunnels | ✅ | `tunnel.rs` (kubectl/ssh -L/cloudflared/ngrok), k8s namespace clusters | core |
| 19 | Shell hook for EADDRINUSE | ❌ | — | — |
| 20 | Idle auto-cleanup policy | ⏭ | deliberately not built: auto-killing is against "explain before act" | — |
| 21 | Close a single TCP connection | ⏭ | needs admin / `INET_DIAG_DESTROY`; low value | — |
| 22 | Team config / cloud sync | ⏭ | non-goal (local-first) | — |

### UX principles

| Item | Status | Where | Tests |
|---|---|---|---|
| Keyboard-first (TUI keys, global hotkey, type-to-filter) | ✅ | TUI `app.rs`, desktop `onKey`, `shortcuts.rs`, `ShortcutsDialog.svelte` | vitest (palette, roving, list), desk |
| Explain before act | ✅ | `ActionPlan` in every confirm (incl. cluster order) | cli, e2e |
| Fast (< 50 ms listing, idle CPU ≈ 0) | 🟡 | measured 58 ms warm on a 260-process box (`bench`); watcher every 4 s visible, 2.5× (≥ 10 s) hidden | bench |
| Honest about permissions | ✅ | hidden counts, `NeedsElevation` | core |
| Local & private | ✅ | no network except Docker socket and opt-in SSH | — |

## 04 — Platform internals

| Item | Status | Where | Tests |
|---|---|---|---|
| Common model (socket → PID → owner) | ✅ | `model.rs`, `provider.rs` traits | core |
| Linux enumeration (`/proc/net/*` + fd inode map) | ✅ | `sys/linux.rs` | e2e |
| Linux namespaces / containers | 🟡 | container mapping via runtime API; no netns walking | core |
| Linux race-free stop (pidfd, start time) | ✅ | `sys/linux.rs`, `exec.rs` | core, e2e |
| Linux privileges (honest hidden sockets) | ✅ | hidden sockets | core |
| Linux events instead of polling | ⏭ | polling with persistent `sysinfo` state; eBPF needs root | — |
| macOS libproc enumeration | ✅ | `sys/macos.rs` | cross-check |
| macOS other-users' sockets | 🟡 | libproc (`sys/netstat_backend.rs`) can't attribute them, so they're counted as hidden; no privileged helper | core |
| macOS stopping / launchd | 🟡 | signals + brew services; launchd only warned | core |
| macOS elevation / signing | ❌ | — (needs certificates and a helper) | — |
| Windows `GetExtendedTcpTable`/`UdpTable` | ✅ | `sys/windows.rs` | cross-check |
| Windows diagnostics (HTTP.sys, excluded ranges, WSL) | ✅ | `windiag.rs`, `os_service.rs` | core |
| Windows stopping (graceful) | 🟡 | `taskkill` (WM_CLOSE) → `TerminateProcess`; no CTRL_BREAK | cross-check |
| Windows signing | ⏭ | needs a certificate | — |
| Container Engine API, labels → compose project | ✅ | `docker.rs` | core |
| Runtime socket discovery (DOCKER_HOST, Desktop, OrbStack, Colima, Podman) | ✅ | `docker.rs::endpoints` | core |
| Container stop (graceful, timeout) | ✅ | `Step::StopContainer` | core |
| Short daemon timeout | ✅ | `docker.rs` | core |
| Catalogue: npm/yarn/pnpm/bun trees | ✅ | `strategies::is_launcher`, `TopologyBuilder::service_root` | core, e2e |
| Catalogue: nodemon / next / vite / webpack | ✅ | `project/framework.rs` signatures | core |
| Catalogue: pm2 | ✅ | `pm2.rs` (`parse_jlist`) | core |
| Catalogue: systemd service / socket | ✅ | `systemd.rs` (`parse_list_sockets`) | core |
| Catalogue: launchd / brew services | 🟡 | `brew.rs`; generic launchd warning | core |
| Catalogue: Windows services | ❌ | — | — |
| Catalogue: Docker/Podman/OrbStack/Colima | ✅ | container strategy | core |
| Catalogue: kubectl / ssh -L / cloudflared / ngrok | ✅ | `tunnel.rs` + process-tree stop of the forwarder | core |
| Catalogue: IDE/agent-spawned (never kill the IDE) | ✅ | `safety.rs` `HOST_MARKERS`, own-tree protection | core, e2e |
| Performance budgets | 🟡 | `benches/scan.rs`: scan 58 ms, topology 0.23 ms, explain 12 µs, 2 000 ss lines 0.76 ms (scan is over the 50 ms target) | bench |

## 05 — Tech stack

| Item | Status | Where |
|---|---|---|
| Rust workspace: core + CLI/TUI + MCP | ✅ | `crates/` |
| Tauri v2 tray app (Svelte 5) | ✅ | `apps/desktop` |
| Crate choices (sysinfo, netstat2, ratatui, clap, …) | ✅ | `Cargo.toml` |
| Distribution matrix (brew, winget, scoop, npm, apt) | 🟡 | cargo-dist workflows; not published |

## 06 — Architecture & plan

| Item | Status | Where | Tests |
|---|---|---|---|
| Core is a library, surfaces are thin | ✅ | see `docs/architecture.md`; desktop `commands.rs` is pure adapters | all |
| Owner ≠ PID; ranked actions | ✅ | `Owner`, `StrategyRegistry` | core |
| Plan → confirm → execute → verify | ✅ | `ActionPlan`, `exec.rs` | e2e |
| No shell-out in the hot path | ✅ | shell-outs only inside strategies, with timeouts | core |
| Least privilege | 🟡 | unprivileged by default; no elevated helper | — |
| Local-first, no telemetry | ✅ | — | — |
| CLI surface (MVP list) | ✅ | `main.rs` | cli |
| Testing: parsers against recorded fixtures | ✅ | `tests/fixtures`, docker/netsh/pm2/systemd fixtures | core |
| Testing: `insta` snapshots | ❌ | explicit assertions instead | — |
| Testing: `proptest` | ✅ | `topology/proptests.rs` (ordering, parsers, targets) | prop |
| Testing: real-OS integration harness | ✅ | `e2e_linux.rs` (trees, SIGTERM-ignorers, connected client→server, cluster stop) | e2e |
| Testing: containers in CI | 🟡 | fixtures only (no daemon here) | core |
| Testing: safety golden tests | ✅ | protected/never-signalled tests | core, e2e |
| Testing: CLI assert_cmd + JSON Schema | 🟡 | assert_cmd ✅; JSON Schema ❌ | cli |
| Testing: GUI component tests + Playwright | 🟡 | vitest for all pure UI logic, plus Testing Library component tests (UI kit, dialogs, list rows); no Playwright end-to-end yet | vitest |
| Testing: criterion + CI regression gate | 🟡 | benches exist; no CI regression gate yet | bench |

## Topology / mesh and SOLID

| Item | Status | Where | Tests |
|---|---|---|---|
| Core `topology` graph (nodes = services, edges = local connections, external collapsed) | ✅ | `topology/{model,builder}.rs` | core, e2e |
| Clusters: compose, k8s namespace, supervisor (pm2/turbo/concurrently/nx), workspace root, git repo | ✅ | `topology/cluster.rs` (`ClusterRegistry`) | core |
| Dependency-ordered stop plan with warnings | ✅ | `topology/order.rs`, `engine/cluster.rs`, `Target::Cluster` | core, prop, e2e |
| `portwise graph` tree + `--json/--dot/--mermaid` | ✅ | `crates/portwise-cli/src/graph.rs`, `topology/export.rs` | cli |
| TUI graph tab | ✅ | `tui/graph.rs`, `tui/graph_ui.rs` | cli (unit) |
| Desktop graph view (hulls, direction, traffic, hover, select-sync, zoom/fit, minimap, layout toggle, themes, reduced motion) | ✅ | `GraphView.svelte`, `components/graph/*`, `lib/graph.ts` | vitest |
| Desktop stop cluster with plan | ✅ | `ConfirmDialog.svelte` (order chips), `App.svelte::requestClusterStop` | vitest (order parsing) |
| List/graph switch + cluster grouping in list | ✅ | `App.svelte`, `lib/graph.ts::sectionsByCluster` | vitest |
| MCP topology tools | ✅ | `get_topology`, `plan_cluster_stop` | mcp |
| `SocketProvider`/`ProcessProvider`/`ContainerProvider` | ✅ | `provider.rs`, `Scanner` | core |
| `ProjectDetector` strategies (open/closed registries) | ✅ | `project/{manifest,workspace}.rs` | core |
| `ProtectionPolicy` | ✅ | `safety.rs` | core |
| `StopStrategy` implementations + registry | ✅ | `engine/strategies/` | core |
| `TopologyBuilder` + `GraphExporter` | ✅ | `topology/` | core, cli |

## Product quality — desktop UX, typography, documentation

Work beyond the research documents, audited with the same rules.

| Item | Status | Where | Tests |
|---|---|---|---|
| UI kit with one set of heights, radii and focus rings, and a `#ui-gallery` page | ✅ | `apps/desktop/src/components/ui/`, `src/dev/Gallery.svelte` | vitest (`components/ui/ui.test.ts`) |
| Settings (launch at login, shortcut presets, theme, density, notifications, scanning, history) | ✅ | `SettingsDialog.svelte`, `lib/settings.ts`, `src-tauri/src/commands.rs` | vitest, desk |
| Typography system (bundled Inter and JetBrains Mono, nine roles) with a lint | ✅ | `app.css`, `fonts.css`, `src/typography.test.ts`, TUI `tui/theme.rs` | vitest |
| WCAG AA contrast in both themes | ✅ | `app.css` tokens, `src/contrast.test.ts`, `lib/frameworks.test.ts` | vitest |
| Port list: fixed grid, badge priority, density, collapsible sections, listbox semantics | ✅ | `components/list/`, `lib/rows.ts` | vitest (`rows.test.ts`, `list.test.ts`) |
| User documentation: README, user guide, MCP guide, FAQ, platform matrix | ✅ | `README.md`, `docs/user-guide.md`, `docs/mcp.md` | docs |
| Contributor documentation: development, releasing, security, code of conduct, templates | ✅ | `CONTRIBUTING.md`, `docs/development.md`, `docs/releasing.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `.github/` | docs |
| Generated CLI reference, man pages and shell completions | ✅ | `cli_docs.rs`, `portwise man --out-dir`, `portwise completions`, `scripts/gen-docs.sh` | docs, cli |
| Docs CI: CLI reference, README commands and flags, links and anchors, orphaned screenshots, `cargo doc -D warnings` | ✅ | `cli_docs.rs`, `tests/repo_conventions.rs`, `.github/workflows/ci.yml` | docs |
| File-naming conventions, documented and enforced | ✅ | `CONTRIBUTING.md` § Naming conventions, `tests/repo_conventions.rs` | docs |
