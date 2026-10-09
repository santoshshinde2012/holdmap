# portwise architecture

portwise is one Rust library (`portwise-core`) with four thin front-ends. Every decision about
*who owns a port, whether it's safe to touch, and how to stop it* lives in the core; the
front-ends only render and confirm what it returns.

## 1. Layers

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 260, "nodeSpacing": 40, "rankSpacing": 50}}}%%
flowchart TB
  subgraph IF["Interfaces"]
    direction LR
    HOOK("Shell hook<br/>portwise init")
    CLI("CLI<br/>clap")
    TUI("TUI<br/>ratatui")
    DESK("Desktop app + tray<br/>Tauri v2 · Svelte 5")
    MCP("MCP server<br/>JSON-RPC · stdio")
  end

  subgraph CORE["portwise-core"]
    ENG{{"Engine<br/>explain · plan · stop"}}
    SCAN["Scanner<br/>sockets → PIDs → projects"]
    PROV["Platform providers<br/>Linux · macOS · Windows"]
    POL["ProtectionPolicy<br/>never touch the OS,<br/>shells, IDEs, agents"]
    REG["StopStrategy registry<br/>process tree · container<br/>systemd · pm2 · brew"]
    EXEC["Executor<br/>signal → verify freed"]
    TOPO["Topology<br/>service graph<br/>clusters · stop order"]
    AGT["Agents<br/>AI coding agents · folders<br/>access · ports · links"]
    HTTP["HTTP probe<br/>GET / → status, title"]
    STACK["Project config<br/>.portwise.toml"]
  end

  subgraph OS["Operating system"]
    direction LR
    SOCK[["Sockets & processes"]]
    SIG[["Signals<br/>SIGTERM → SIGKILL"]]
    CTR[["Containers<br/>Docker · Podman<br/>OrbStack · Colima"]]
  end

  subgraph ST["Local state"]
    direction LR
    PINS[("Pins & settings")]
    HIST[("Stop history")]
  end

  HOOK -->|"port taken?"| CLI

  IF ==>|"scan · explain · stop"| ENG
  IF -.->|"HTTP status"| HTTP
  IF --> ST
  CLI -->|"up · down"| STACK
  STACK --> ENG

  ENG --> SCAN
  SCAN --> PROV
  ENG -->|"safe to touch?"| POL
  ENG -->|"how to stop"| REG
  ENG --> TOPO
  ENG --> AGT
  AGT -.->|"same catalog"| POL
  REG --> EXEC

  PROV -->|"read"| SOCK
  SCAN -->|"published ports"| CTR
  EXEC -->|"send"| SIG
  EXEC -->|"stop"| CTR
  HTTP -.->|"localhost"| SOCK

  classDef iface fill:#dbeafe,stroke:#2563eb,stroke-width:1.5px,color:#0b1b3a
  classDef core fill:#ede9fe,stroke:#7c3aed,stroke-width:1.5px,color:#1e1035
  classDef engine fill:#7c3aed,stroke:#5b21b6,stroke-width:2px,color:#ffffff
  classDef os fill:#dcfce7,stroke:#16a34a,stroke-width:1.5px,color:#052e16
  classDef state fill:#fef3c7,stroke:#d97706,stroke-width:1.5px,color:#3b2203
  class HOOK,CLI,TUI,DESK,MCP iface
  class SCAN,PROV,POL,REG,EXEC,TOPO,AGT,HTTP,STACK core
  class ENG engine
  class SOCK,SIG,CTR os
  class PINS,HIST state
  style IF fill:transparent,stroke:#60a5fa,stroke-dasharray:4 3
  style CORE fill:transparent,stroke:#a78bfa,stroke-dasharray:4 3
  style OS fill:transparent,stroke:#4ade80,stroke-dasharray:4 3
  style ST fill:transparent,stroke:#f59e0b,stroke-dasharray:4 3
```

Solid arrows are calls; the thick one is the API every interface shares, and dotted arrows are the
optional HTTP probe.

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

### Agents

`agents::AgentsBuilder` turns the same `Scan` into an `AgentsReport`:

* **Who.** `agents::catalog` names each agent product: bundle, process name, install path or
  entry script. Kinds cover terminal agents, AI editors, desktop apps, extensions, hosts and
  developer tools (Docker Desktop, OrbStack…). `ProtectionPolicy` uses the same catalog for
  Cli/Extension/Host names, so what the view calls an agent is what portwise refuses to stop. An
  editor, desktop app or tool with many helpers is one agent. A terminal agent started inside it
  is its own agent, linked by a parent edge.
* **Where.** Folders are the members' working directories, with project details from the scan,
  plus recent projects from a `RecentProjects` source. It reads only directory names under
  `~/.claude/projects` and the window folder URIs in the editors' `storage.json`. It never opens
  chats, settings or tokens.
* **Reach.** Listening ports held by the agent or its children, and ESTABLISHED links grouped by
  local service or remote `ip:port` (no DNS).
* **Access.** Each fact (account, sandbox, approvals, network, privacy) carries `observed`,
  `inferred` or `unknown`. Folders inside macOS privacy-protected areas are listed but not read.

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
    CMD["commands.rs<br/>scan · topology · agents · explain · plan · stop · pins · history · restart · autostart<br/>preferences · hotkeys · remote_scan"]
    ST["state.rs<br/>AppState (config, last snapshot)"]
    TR["tray.rs<br/>menu + top ports"]
    W["watch.rs<br/>scan interval (default 4 s) / 2.5× hidden → events → notifications + tray"]
    SC["shortcuts.rs<br/>presets, re-registered at runtime"]
    LIB --> CMD & TR & W & SC
    CMD & TR & W --> ST
  end
  subgraph UI["Svelte 5 front-end"]
    APP["App.svelte<br/>list ⇄ graph (G) ⇄ agents (⇧A), palette, keys"]
    GV["GraphView.svelte<br/>@xyflow/svelte"]
    AV["AgentsView.svelte<br/>agent cards + footprint map<br/>(lib/agents.ts: column layout)"]
    NODES["components/graph/<br/>ServiceNode · ClusterNode · TrafficEdge · FitOnChange"]
    LG["lib/graph.ts<br/>dagre layered · force · toFlow · related · sections"]
    OTHER["detail/DetailPane · Settings · Pin · Remote · Confirm · History"]
    LIST["components/list/<br/>PortRow · GroupHeader · StatusDot · RowBadges · Sparkline<br/>(view model: lib/rows.ts)"]
    KIT["components/ui/<br/>UI kit (see below)"]
    API["lib/api.ts (invoke) · lib/mock.ts (browser dev)"]
    APP --> GV --> NODES
    APP --> AV --> NODES
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
- Project and shell features stay out of the engine: `stack` parses and validates
  `.portwise.toml` and answers "is this listener ours?" (pure data, no I/O beyond reading the
  file), `hint` reads a command line for the ports it would bind, and `http` probes a port with a
  bounded `GET /`. The CLI composes them with the engine's normal plan → confirm → execute path,
  so `down`, `up --replace` and `stop --all-dev` get the same protection checks as `stop`.
- `docs/cli.md`, the man pages and the shell completions are all generated from the clap
  definitions; `cargo test` fails when `docs/cli.md` is stale.
