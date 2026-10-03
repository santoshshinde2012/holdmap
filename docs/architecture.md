# portwise architecture

portwise is one Rust library (`portwise-core`) with four thin front-ends. Every decision about
*who owns a port, whether it's safe to touch, and how to stop it* lives in the core; the
front-ends only render and confirm what it returns.

## 1. Layers

```mermaid
flowchart TB
  subgraph Frontends
    CLI["portwise CLI<br/>crates/portwise-cli"]
    TUI["TUI (ratatui)<br/>portwise-cli/src/tui"]
    DESK["Desktop (Tauri v2 + Svelte 5)<br/>apps/desktop"]
    MCP["MCP server<br/>crates/portwise-mcp"]
  end
  subgraph Core["portwise-core"]
    ENG["Engine<br/>explain · plan · plan_cluster · topology"]
    SCAN["Scanner"]
    TOPO["topology::TopologyBuilder"]
    EXEC["exec (signal → verify)"]
    STORE["store / history / events / remote"]
  end
  subgraph OS["OS + runtimes"]
    PROC["/proc · libproc · IP Helper"]
    DOCK["Docker / Podman / OrbStack / Colima API"]
    SUP["systemd · pm2 · brew · kubectl"]
  end
  CLI --> ENG
  TUI --> ENG
  DESK --> ENG
  MCP --> ENG
  CLI & TUI & DESK --> STORE
  ENG --> SCAN --> PROC & DOCK
  ENG --> TOPO
  ENG --> EXEC --> PROC & DOCK & SUP
```

## 2. Scan → plan → execute

The core is built from small traits so each piece can be swapped (real OS, recorded fixture, or a
fake in a test) without touching the others (dependency inversion).

```mermaid
flowchart LR
  SP["SocketProvider<br/>SystemSockets · StaticSockets"] --> S
  PP["ProcessProvider<br/>SystemProcesses · StaticProcesses"] --> S
  CP["ContainerProvider<br/>DockerContainers · StaticContainers"] --> S
  S["Scanner"] --> SNAP["Scan<br/>(Snapshot + ProcessTable)"]
  SNAP --> E["Engine"]
  POL["ProtectionPolicy<br/>DefaultProtectionPolicy"] --> E
  REG["StrategyRegistry<br/>Vec&lt;Box&lt;dyn StopStrategy&gt;&gt;"] --> E
  E --> PLAN["ActionPlan<br/>steps + warnings + blocked"]
  PLAN --> X["exec::execute"]
  X --> V["verify port freed"]
```

`StrategyRegistry` asks each `StopStrategy` in priority order whether it applies to an owner and
takes the first that does:

| Strategy | Handles |
|---|---|
| `container` | docker-proxy / published container ports → stop the container |
| `hidden` | socket whose owner we can't see → "needs elevation" |
| `os_service` | AirPlay, HTTP.sys, wslrelay … → explain, never kill |
| `systemd` | unit or `.socket` activation → `systemctl stop` |
| `pm2` | pm2-managed app → `pm2 stop` |
| `brew` | `brew services` → `brew services stop` |
| `protected` | own tree, IDE/AI-assistant hosts, interactive shells, PID 1 … → blocked |
| `other_user` | another user's process → needs elevation |
| `process_tree` | fallback: launcher tree (npm → node → esbuild …) bottom-up SIGTERM → SIGKILL |

Supervisors come before `protected` on purpose: a pm2 app or a systemd unit must be stopped
through its supervisor even though its process would also match a later rule. The table is the
order of `StrategyRegistry::default()` in `engine/strategies/mod.rs`.

Adding a supervisor means adding one `impl StopStrategy` and registering it. `Engine`, the CLI
and the UIs don't change (open/closed).

### Stop sequence

```mermaid
sequenceDiagram
  participant U as User / MCP client
  participant F as Front-end
  participant E as Engine
  participant X as exec
  participant OS
  U->>F: stop :3000
  F->>E: plan(Target::Port(3000))
  E->>E: Scanner.scan() → owner → ProtectionPolicy → StopStrategy
  E-->>F: ActionPlan (steps, warnings, blocked?)
  F->>U: show plan, confirm
  U->>F: yes
  F->>X: execute(plan)
  loop each step
    X->>OS: re-check start token (PID reuse guard, pidfd on Linux)
    X->>OS: SIGTERM / container stop / systemctl stop
    X->>OS: wait grace, then SIGKILL if allowed
  end
  X->>OS: is the port free?
  X-->>F: StopReport
  F->>F: Store::record (history, for restart)
```

## 3. Topology / mesh

```mermaid
flowchart LR
  SNAP["Scan<br/>(all states)"] --> TB["TopologyBuilder"]
  TB -->|"listeners → service roots"| N["nodes"]
  TB -->|"ESTABLISHED pairs → edges"| ED["edges (+ external, collapsed)"]
  TB --> TUN["tunnel::detect<br/>kubectl · ssh -L · cloudflared · ngrok"]
  CR["ClusterRegistry<br/>Compose → Kubernetes → Supervisor → Workspace → Git"] --> TB
  N & ED & TUN --> G["Graph"]
  G --> EXP["GraphExporter<br/>Json · Dot · Mermaid · Tree"]
  G --> ORD["stop_order<br/>(dependents first, cycle-safe)"]
  ORD --> PC["Engine::plan_cluster → ActionPlan"]
  G --> UI["TUI graph tab · desktop GraphView · MCP get_topology"]
```

* **Nodes** are services, not PIDs: a listener is folded up to its service root (the launcher
  tree), so `npm run dev → node → esbuild` is one node with all its ports.
* **Edges** come from ESTABLISHED sockets where both ends are local; traffic weight is the
  connection count. Remote peers collapse into one `external` node per host.
* **Clusters** come from the first `ClusterDetector` that claims a node (compose project label,
  k8s namespace of a port-forward, pm2/turbo/concurrently/nx parent, workspace root, git repo).
* **Stop order**: callers before callees (`web → api → db`), so nothing reconnects to a
  dependency mid-shutdown. The order is deterministic, which proptest checks.
  Dependents outside the cluster become warnings in the plan.

## 4. Project detection

```mermaid
flowchart LR
  CWD["process cwd"] --> PD["ProjectDetector"]
  PD --> MR["ManifestRegistry<br/>Node · Cargo · PyProject · GoMod · Composer · Django manage.py · Generic (Gemfile, pom.xml, mix.exs, deno.json …)"]
  PD --> WM["WorkspaceMarker[]<br/>pnpm · turbo · nx · lerna · npm workspaces · cargo workspace · go.work · compose file"]
  PD --> GIT["git root + branch"]
  MR & WM & GIT --> PDT["ProjectDetails"]
  PDT --> FW["detect_framework<br/>(signature table)"]
```

## 5. Desktop app

```mermaid
flowchart TB
  subgraph Rust["src-tauri (portwise-desktop)"]
    LIB["lib.rs: builder, plugins<br/>(notification, autostart, global-shortcut)"]
    CMD["commands.rs<br/>scan · topology · explain · plan · stop · pins · history · restart · autostart<br/>preferences · hotkeys · remote_scan"]
    ST["state.rs<br/>AppState (config, last snapshot)"]
    TR["tray.rs<br/>menu + top ports"]
    W["watch.rs<br/>scan interval (default 4 s) / 2.5× hidden → events → notifications + tray"]
    SC["shortcuts.rs<br/>presets, re-registered at runtime"]
    LIB --> CMD & TR & W & SC
    CMD & TR & W --> ST
  end
  subgraph UI["Svelte 5 front-end"]
    APP["App.svelte<br/>list ⇄ graph (G), palette, keys"]
    GV["GraphView.svelte<br/>@xyflow/svelte"]
    NODES["components/graph/<br/>ServiceNode · ClusterNode · TrafficEdge · FitOnChange"]
    LG["lib/graph.ts<br/>dagre layered · force · toFlow · related · sections"]
    OTHER["detail/DetailPane · Settings · Pin · Remote · Confirm · History"]
    LIST["components/list/<br/>PortRow · GroupHeader · StatusDot · RowBadges · Sparkline<br/>(view model: lib/rows.ts)"]
    KIT["components/ui/<br/>UI kit (see below)"]
    API["lib/api.ts (invoke) · lib/mock.ts (browser dev)"]
    APP --> GV --> NODES
    GV --> LG
    APP --> OTHER --> KIT
    APP --> LIST --> KIT
    APP --> API
  end
  API -- "Tauri IPC" --> CMD
```

`commands.rs` only converts arguments and calls the core. All behaviour lives in `portwise-core`,
and all view logic (layout, filtering, ordering) lives in pure TypeScript modules that vitest
covers.

### Desktop UI

- **UI kit.** `apps/desktop/src/components/ui/` is the only place that styles controls (fields,
  selects, switches, buttons, dialogs, tabs…); feature components compose it. Open the app with
  `#ui-gallery` to see every control in both themes.
- **Pure view logic.** `lib/rows.ts` (badges, status, sparklines), `lib/detail.ts` (details pane)
  and `lib/settings.ts` (the Settings contract) are pure and unit-tested, so components depend on
  interfaces rather than on `api.ts`.
- **Accessibility.** The list is a `role=listbox` with `aria-activedescendant`, dialogs trap and
  restore focus, and `src/contrast.test.ts` keeps text tokens at WCAG AA in both themes.
- **Typography.** Inter Variable for UI text and JetBrains Mono Variable for code, PIDs and paths,
  bundled locally (SIL OFL 1.1). One scale (11–32 px, base 13 px, weights 400/500/600) exposed as
  semantic tokens in `app.css`; `src/typography.test.ts` rejects raw values. The TUI and CLI map the
  same roles onto bold and colour.

## 6. Graph library

The graph view uses **Svelte Flow (`@xyflow/svelte`) and `@dagrejs/dagre`**. Nodes are real DOM
(so they reuse the icons, tokens and focus rings), group nodes draw the cluster hulls, and the
minimap and zoom controls are built in. dagre gives a stable left-to-right layout that matches the
stop order; the alternative "organic" layout is a small seeded force simulation, so the graph
doesn't move between refreshes.

## 7. Design notes

- `scan` gathers, `engine` decides, `exec` acts, `topology` relates and `store` persists. The
  desktop backend is split into `commands`, `state`, `tray`, `watch` and `shortcuts`.
- Behaviour is extended through registries: `StrategyRegistry`, `ClusterRegistry`,
  `ManifestRegistry`, `WorkspaceMarker`s and the `exporter(name)` factory.
- `Engine` and `Scanner` take trait objects, so tests inject static providers and fixture tables.
  Only `Scanner::system()` (used by `Engine::new`) touches the real OS.
- `docs/cli.md`, the man pages and the shell completions are all generated from the clap
  definitions; `cargo test` fails when `docs/cli.md` is stale.
