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
| `protected` | own tree, IDE/agent hosts, interactive shells, PID 1 … → blocked |
| `other_user` | another user's process → needs elevation |
| `systemd` | unit or `.socket` activation → `systemctl stop` |
| `pm2` | pm2-managed app → `pm2 stop` |
| `brew` | `brew services` → `brew services stop` |
| `process_tree` | fallback: launcher tree (npm → node → esbuild …) bottom-up SIGTERM → SIGKILL |

Adding a supervisor means adding one `impl StopStrategy` and registering it. `Engine`, the CLI
and the UIs don't change (open/closed).

### Stop sequence

```mermaid
sequenceDiagram
  participant U as User / agent
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
  X-->>F: StopOutcome
  F->>F: history::record (for restart)
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
  PD --> MR["ManifestRegistry<br/>Node · Cargo · PyProject · GoMod · Composer · Django · Generic"]
  PD --> WM["WorkspaceMarker[]<br/>pnpm · turbo · nx · lerna · cargo workspace · go.work"]
  PD --> GIT["git root + branch"]
  MR & WM & GIT --> PDT["ProjectDetails"]
  PDT --> FW["detect_framework<br/>(signature table)"]
```

## 5. Desktop app

```mermaid
flowchart TB
  subgraph Rust["src-tauri (portwise-desktop)"]
    LIB["lib.rs: builder, plugins<br/>(notification, autostart, global-shortcut)"]
    CMD["commands.rs<br/>scan · topology · explain · plan · stop · pins · history · restart · autostart"]
    ST["state.rs<br/>AppState (config, last snapshot)"]
    TR["tray.rs<br/>menu + top ports"]
    W["watch.rs<br/>4 s visible / 10 s hidden → events → notifications + tray"]
    SC["shortcuts.rs<br/>Ctrl+Alt+P / ⌘⌥P"]
    LIB --> CMD & TR & W & SC
    CMD & TR & W --> ST
  end
  subgraph UI["Svelte 5 front-end"]
    APP["App.svelte<br/>list ⇄ graph (G), palette, keys"]
    GV["GraphView.svelte<br/>@xyflow/svelte"]
    NODES["components/graph/<br/>ServiceNode · ClusterNode · TrafficEdge · FitOnChange"]
    LG["lib/graph.ts<br/>dagre layered · force · toFlow · related · sections"]
    OTHER["DetailPane · ConfirmDialog · HistoryPanel · PortRow"]
    API["lib/api.ts (invoke) · lib/mock.ts (browser dev)"]
    APP --> GV --> NODES
    GV --> LG
    APP --> OTHER
    APP --> API
  end
  API -- "Tauri IPC" --> CMD
```

`commands.rs` only converts arguments and calls the core. All behaviour lives in `portwise-core`,
and all view logic (layout, filtering, ordering) lives in pure TypeScript modules that vitest
covers.

## 6. Graph library choice

**Svelte Flow (`@xyflow/svelte` 1.x) + `@dagrejs/dagre`**, with our own small deterministic force
simulation for the "organic" layout.

| Need | Svelte Flow | Cytoscape.js | d3-force + hand-rolled SVG |
|---|---|---|---|
| Svelte 5 native | ✅ | wrapper | manual |
| Nodes are real DOM (reuse `FrameworkIcon`, CSS tokens, focus rings) | ✅ | ❌ canvas | ✅ |
| Group/parent nodes for cluster hulls | ✅ | ✅ compound | manual |
| Custom animated edges (direction + traffic dots via `animateMotion`) | ✅ | limited | ✅ |
| MiniMap, Controls, Background, fit/zoom | built-in | plugins | manual |
| Dark mode (`colorMode`) | ✅ | manual | manual |
| Bundle cost | moderate | heavier | small, but most work is ours |

dagre gives a stable layered (left-to-right, callers → callees) layout that matches the stop
order. Isolated services are placed on a grid instead of one long row. The force layout is
seeded and deterministic so the graph doesn't move between refreshes. That is why there's no
d3 dependency.

## 7. SOLID notes

* **S**: `scan` gathers, `engine` decides, `exec` acts, `topology` relates, `store` persists.
  The desktop backend is split by concern (`commands`/`state`/`tray`/`watch`/`shortcuts`).
* **O**: four open registries: `StrategyRegistry`, `ClusterRegistry`, `ManifestRegistry` and
  `WorkspaceMarker`s, plus the `exporter(name)` factory. New behaviour is a new impl.
* **L**: every `StopStrategy` returns the same `Resolution` contract. Every exporter takes a
  `&Graph` and returns a `String`.
* **I**: the providers are three narrow traits rather than one "system" trait. `ProtectionPolicy`
  is a single method set.
* **D**: `Engine` and `Scanner` take trait objects. Tests inject `Static*` providers and fixture
  tables (see `engine` tests, `topology/tests.rs`, `benches/scan.rs`). Only `Engine::system()`
  wires the real OS.
