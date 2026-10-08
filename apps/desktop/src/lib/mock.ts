// Realistic sample data so the UI can be developed in a plain browser (`npm run dev`) and tested.
import type { ActionPlan, BindRisk, Cluster, Explanation, Graph, GraphEdge, GraphNode, HttpInfo, PortDetails, PortEntry, Snapshot, StopReport } from "./types";

const now = Math.floor(Date.now() / 1000);
let token = 1000;

function entry(p: Partial<PortEntry> & { port: number }): PortEntry {
  const pid = p.pid === undefined ? 40000 + p.port : p.pid;
  return {
    id: `${p.protocol ?? "tcp"}:${p.port}:${pid}:LISTEN`,
    protocol: "tcp",
    state: p.protocol === "udp" ? "bound" : "listen",
    addresses: ["127.0.0.1", "::1"],
    families: ["v4", "v6"],
    remote: null,
    exposure: "loopback",
    pid,
    pids: pid ? [pid] : [],
    uid: 501,
    user: "dev",
    process: null,
    project: null,
    framework: null,
    container: null,
    label: "",
    is_dev: false,
    is_mine: true,
    protected: false,
    ...p,
  };
}

function proc(pid: number, name: string, cmd: string, cwd: string | null, ageSecs: number, mem: number) {
  return {
    pid,
    ppid: 1,
    name,
    exe: null,
    cmdline: cmd.split(" "),
    cwd,
    uid: 501,
    user: "dev",
    start_time: now - ageSecs,
    start_token: token++,
    memory_bytes: mem,
  };
}

export const MOCK_SNAPSHOT: Snapshot = {
  platform: "macos",
  taken_at_ms: Date.now(),
  scan_ms: 38,
  docker_available: true,
  hidden_sockets: 1,
  warnings: [],
  entries: [
    entry({
      port: 3000,
      process: proc(43000, "node", "node /Users/dev/code/shop-web/node_modules/.bin/next dev", "/Users/dev/code/shop-web", 7420, 412e6),
      project: { name: "shop-web", root: "/Users/dev/code/shop-web", kind: "package.json", git_branch: "feat/checkout" },
      framework: { name: "Next.js", category: "dev_server" },
      label: "Next.js · shop-web (feat/checkout)",
      is_dev: true,
      app_memory_bytes: 1.18e9,
      helper_count: 3,
    }),
    entry({
      port: 3001,
      process: proc(43001, "node", "node server/index.js", "/Users/dev/code/shop-api", 7300, 96e6),
      project: { name: "shop-api", root: "/Users/dev/code/shop-api", kind: "package.json", git_branch: "main" },
      framework: { name: "Express", category: "app_server" },
      label: "Express · shop-api (main)",
      is_dev: true,
    }),
    entry({
      port: 5173,
      addresses: ["::1"],
      families: ["v6"],
      process: proc(45173, "node", "node node_modules/.bin/vite dev", "/Users/dev/code/docs", 900, 180e6),
      project: { name: "docs", root: "/Users/dev/code/docs", kind: "package.json", git_branch: "main" },
      framework: { name: "Vite", category: "dev_server" },
      label: "Vite · docs (main)",
      is_dev: true,
    }),
    entry({
      port: 8000,
      addresses: ["0.0.0.0"],
      families: ["v4"],
      exposure: "all_interfaces",
      process: proc(48000, "Python", "python manage.py runserver 0.0.0.0:8000", "/Users/dev/code/ml-service", 86400 * 2, 140e6),
      project: { name: "ml-service", root: "/Users/dev/code/ml-service", kind: "manage.py", git_branch: "exp/embeddings" },
      framework: { name: "Django", category: "dev_server" },
      label: "Django · ml-service (exp/embeddings)",
      is_dev: true,
    }),
    entry({
      port: 5432,
      addresses: ["0.0.0.0", "::"],
      exposure: "all_interfaces",
      process: proc(1290, "com.docker.backend", "com.docker.backend", null, 86400 * 3, 300e6),
      container: { id: "8f2c1d", name: "shop-db-1", image: "postgres:16", runtime: "Docker", compose_project: "shop", compose_service: "db", private_port: 5432 },
      framework: { name: "PostgreSQL", category: "database" },
      label: "PostgreSQL · container shop-db-1",
      is_dev: true,
    }),
    entry({
      port: 6379,
      process: proc(611, "redis-server", "/opt/homebrew/opt/redis/bin/redis-server 127.0.0.1:6379", "/", 86400 * 6, 12e6),
      framework: { name: "Redis", category: "cache" },
      label: "Redis",
    }),
    entry({
      port: 11434,
      process: proc(835, "ollama", "/Applications/Ollama.app/Contents/Resources/ollama serve", "/", 86400 * 4, 2.6e9),
      framework: { name: "Ollama", category: "tool" },
      label: "Ollama",
    }),
    entry({
      port: 8125,
      protocol: "udp",
      process: proc(48125, "statsd", "node stats.js", "/Users/dev/code/metrics", 4000, 30e6),
      framework: { name: "Node.js", category: "app_server" },
      label: "Node.js · metrics",
      is_dev: true,
    }),
    entry({
      port: 5000,
      addresses: ["0.0.0.0", "::"],
      exposure: "all_interfaces",
      process: proc(522, "ControlCenter", "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter", "/", 86400 * 6, 60e6),
      framework: { name: "AirPlay Receiver", category: "system" },
      label: "AirPlay Receiver (macOS)",
      protected: true,
    }),
    entry({
      port: 49152,
      process: proc(912, "Code Helper", "Code Helper (Plugin)", "/", 30000, 220e6),
      framework: { name: "VS Code", category: "app" },
      label: "VS Code",
      protected: true,
    }),
    entry({ port: 631, pid: null, uid: 0, user: "root", is_mine: false, label: "hidden (owned by root)" }),
  ],
};

export function mockExplain(port: number): Explanation {
  const e = MOCK_SNAPSHOT.entries.find((x) => x.port === port);
  if (!e) {
    return { port, status: "free", owners: [{ kind: "free" }], headline: `Port ${port} is free.`, details: [], recommendation: "Nothing to do — you can start your server on this port.", commands: [], entries: [], plan: null };
  }
  const plan = mockPlan(String(port), false);
  const p = e.process;
  return {
    port,
    status: "busy",
    owners: plan.owners,
    headline: p
      ? `Port ${port} is held by ${e.framework?.name ?? p.name} (${p.name}, PID ${p.pid})${e.project ? ` in ~/code/${e.project.name}` : ""}.`
      : `Port ${port} is held by a process owned by ${e.user}.`,
    details: [
      e.exposure === "all_interfaces"
        ? `Listening on all interfaces (${e.addresses.join(", ")}): reachable from other devices on your network.`
        : `Listening on ${e.addresses.join(", ")} (this machine only).`,
      ...(p ? [`Process: ${p.name} (PID ${p.pid}), user ${p.user}`, `Command: ${p.cmdline.join(" ")}`] : []),
      ...(e.container ? [`Published by ${e.container.runtime} container ${e.container.name} (${e.container.image}).`] : []),
    ],
    recommendation: plan.blocked ? plan.blocked.message : plan.summary,
    commands: [`portwise stop ${port}`, ...(p ? [`kill -TERM ${p.pid}`] : [])],
    entries: [e],
    plan,
  };
}

/** Dev servers "Stop all dev servers" would include: yours, not protected, not containers. */
export function mockDevServers(s: Snapshot = MOCK_SNAPSHOT): PortEntry[] {
  return s.entries.filter((e) => e.is_dev && e.is_mine && !e.protected && !e.container && e.process && e.state === "listen");
}

/** Browser preview: web-ish ports answer with a title, databases don't speak HTTP. */
export function mockHttp(port: number): HttpInfo | null {
  const e = MOCK_SNAPSHOT.entries.find((x) => x.port === port);
  if (!e || e.protocol !== "tcp" || e.framework?.category === "database" || e.framework?.category === "cache") return null;
  return { port, status: 200, reason: "OK", title: e.project?.name ?? e.framework?.name ?? null, server: null, location: null, elapsed_ms: 3 };
}

/** Browser preview of the lazily loaded details: a believable tree, peers and bind risk. */
export function mockDetails(port: number): PortDetails[] {
  return MOCK_SNAPSHOT.entries.filter((e) => e.port === port && (e.state === "listen" || e.protocol === "udp")).map((e) => {
    const p = e.process;
    const exposed = e.exposure === "all_interfaces";
    const data = ["database", "cache", "queue"].includes(e.framework?.category ?? "");
    const risk: BindRisk = exposed
      ? { level: data || e.is_dev ? "high" : "medium", title: "Anyone on your network can connect", explanation: `It listens on ${e.addresses.join(", ")} (every network interface). ${data ? "Databases and caches often accept connections without a password in development, so anyone on the same Wi-Fi could read or change your data." : "Dev servers rarely ask for a login and can expose source files, debug pages and admin routes to anyone on the same Wi-Fi (a café, an office, a conference)."}`, fix: e.container ? `Publish it on loopback only: \`-p 127.0.0.1:${e.port}:${e.container.private_port}\`.` : "Start it with `--host 127.0.0.1` (or `HOST=127.0.0.1`) unless you're testing from another device." }
      : { level: "low", title: "Only this computer can connect", explanation: `It listens on ${e.addresses.join(", ")} (loopback), so other devices on your network can't reach it.`, fix: null };
    const peers = e.protocol === "tcp" ? (port % 3 === 0 ? [{ address: "127.0.0.1", connections: 3, local: true, process: "Google Chrome", pid: 812 }, { address: "192.168.1.24", connections: 1, local: false, process: null, pid: null }] : port % 3 === 1 ? [{ address: "127.0.0.1", connections: 2, local: true, process: "node", pid: 4310 }] : []) : [];
    const total = peers.reduce((n, x) => n + x.connections, 0);
    const node = (pid: number, name: string, command: string, depth: number, mem = 0) => ({ pid, name, command, depth, memory_bytes: mem, cpu_percent: 0 });
    return {
      id: e.id, port: e.port, started_at: p?.start_time ?? null, uptime_secs: p ? Math.round(Date.now() / 1000 - p.start_time) : null,
      connections: { total, established: total, by_state: (total ? { established: total } : {}) as Record<string, number>, peers, more_peers: 0 },
      tree: p ? { ancestors: [node(1, "launchd", "/sbin/launchd", 3), node(p.pid - 40, "zsh", "-zsh", 2), node(p.pid - 2, "npm", "npm run dev", 1, 48_000_000)], process: node(p.pid, p.name, p.cmdline.join(" "), 0, p.memory_bytes), children: e.is_dev ? [node(p.pid + 3, "esbuild", "esbuild --service=0.21.5 --ping", 1, 21_000_000)] : [], more_children: 0 } : null,
      bind_risk: risk,
    };
  });
}

export function mockPlan(target: string, force: boolean, allowProtected = false): ActionPlan {
  if (target === "dev:all") {
    const dev = mockDevServers();
    if (!dev.length) return { target: "all dev servers", owners: [], summary: "", steps: [], blocked: { kind: "nothing_to_stop", message: "No dev servers of yours are running." }, warnings: [], risk: "low" };
    const processes = dev.map((e, i) => ({ pid: e.process!.pid, name: e.process!.name, start_token: i + 1, command: e.process!.cmdline.join(" ") }));
    return {
      target: "all dev servers",
      owners: dev.map((e) => ({ kind: "process", pid: e.process!.pid, name: e.process!.name })),
      summary: `Stop ${dev.length} dev servers: ${dev.map((e) => `:${e.port} ${e.label}`).join(", ")}.`,
      steps: [{ action: "signal_processes", processes, force, timeout_ms: 5000 }, ...dev.map((e) => ({ action: "verify_free" as const, port: e.port, protocol: e.protocol, timeout_ms: 3000 }))],
      blocked: null, warnings: [], risk: "medium",
    };
  }
  const port = parseInt(target, 10);
  const e = MOCK_SNAPSHOT.entries.find((x) => x.port === port)!;
  if (!e?.process) {
    return { target: `:${port}`, owners: [{ kind: "hidden", uid: 0, user: "root" }], summary: "", steps: [], blocked: { kind: "needs_elevation", message: `The owner of port ${port} belongs to user root; run \`sudo portwise stop ${port}\`.` }, warnings: [], risk: "high" };
  }
  if (e.protected && e.framework?.category !== "system" && !allowProtected) {
    return { target: `:${port}`, owners: [{ kind: "protected", pid: e.process.pid, name: e.process.name, reason: "IDE host" }], summary: "", steps: [], blocked: { kind: "protected", message: `${e.process.name} (PID ${e.process.pid}) is part of ${e.framework?.name ?? "an app"} — stopping it can close your editor windows.`, overridable: true }, warnings: [], risk: "high" };
  }
  if (e.protected && e.framework?.category === "system") {
    return { target: `:${port}`, owners: [{ kind: "protected", pid: e.process.pid, name: e.process.name, reason: "part of macOS" }], summary: "", steps: [], blocked: { kind: "os_service", message: `${e.label} is a macOS feature. Turn it off in System Settings → General → AirDrop & Handoff → AirPlay Receiver.` }, warnings: [], risk: "high" };
  }
  if (e.container) {
    return { target: `:${port}`, owners: [{ kind: "container", container: e.container, forwarder_pid: e.process.pid }], summary: `Stop Docker container ${e.container.name} and verify port ${port} is free.`, steps: [
      { action: "stop_container", id: e.container.id, name: e.container.name, runtime: "Docker", endpoint: "unix:///var/run/docker.sock", timeout_s: 10 },
      { action: "verify_free", port, protocol: "tcp", timeout_ms: 3000 },
    ], blocked: null, warnings: ["The container may be restarted by its restart policy or by docker compose."], risk: "low" };
  }
  const procs = [{ pid: e.process.pid - 1, name: "npm", start_token: 1, command: "npm run dev" }, { pid: e.process.pid, name: e.process.name, start_token: 2, command: e.process.cmdline.join(" ") }];
  return {
    target: `:${port}`,
    owners: [{ kind: "process_tree", root_pid: procs[0].pid, root_name: "npm", pids: procs.map((p) => p.pid) }],
    summary: force ? `Force-kill npm run dev (2 processes) and verify port ${port} is free.` : `Gracefully stop npm run dev (2 processes: SIGTERM, then SIGKILL after 5s) and verify port ${port} is free.`,
    steps: [
      { action: "signal_processes", processes: procs, force, timeout_ms: 5000 },
      { action: "verify_free", port, protocol: e.protocol, timeout_ms: 3000 },
    ],
    blocked: null,
    warnings: [],
    risk: allowProtected ? "high" : e.is_dev ? "low" : "medium",
  };
}

export function mockStop(target: string): StopReport {
  if (target === "dev:all") {
    const dev = mockDevServers();
    for (const e of dev) MOCK_SNAPSHOT.entries.splice(MOCK_SNAPSHOT.entries.indexOf(e), 1);
    return { target: "all dev servers", success: true, freed: true, ports_still_busy: [], signalled: dev.map((e) => e.process!.pid), escalated: false, survivors: [], elapsed_ms: 640, log: dev.map((e) => `port ${e.port} is free`), error: null };
  }
  const port = parseInt(target, 10);
  const i = MOCK_SNAPSHOT.entries.findIndex((x) => x.port === port);
  if (i >= 0) MOCK_SNAPSHOT.entries.splice(i, 1);
  return { target: `:${port}`, success: true, freed: true, ports_still_busy: [], signalled: [40000 + port], escalated: false, survivors: [], elapsed_ms: 412, log: ["SIGTERM → npm (42999), node (43000)", "all processes exited after 0.4s", `port ${port} is free`], error: null };
}

/** Browser-preview topology: shop-web → shop-api → db/redis (compose "shop"), docs → api,
 *  ml-service → external. Built from MOCK_SNAPSHOT so ids line up with the list. */
export function mockTopology(s: Snapshot = MOCK_SNAPSHOT): Graph {
  const byPort = (p: number) => s.entries.find((e) => e.port === p)!;
  const svc = (port: number, id: string, label: string, cluster: string | null, kind: GraphNode["kind"] = "service"): GraphNode => {
    const e = byPort(port);
    return {
      id, kind: e.container ? "container" : kind, label, subtitle: e.label, root_pid: e.pid, pids: e.pids.length ? e.pids : e.pid ? [e.pid] : [],
      ports: [{ port, protocol: e.protocol, exposure: e.exposure, entry_id: e.id }], framework: e.framework, project: e.project?.name ?? null,
      project_root: e.project?.root ?? null, container: e.container, tunnel: null, cluster, cpu_percent: (port % 7) * 0.9,
      memory_bytes: e.process?.memory_bytes ?? 0, is_dev: e.is_dev, protected: e.protected,
    };
  };
  const nodes: GraphNode[] = [
    svc(3000, "web", "shop-web", "shop"),
    svc(3001, "api", "shop-api", "shop"),
    svc(5432, "db", "db", "shop"),
    svc(6379, "redis", "redis", "shop"),
    svc(5173, "docs", "docs", null),
    svc(8000, "ml", "ml-service", null),
    { id: "external", kind: "external", label: "External", subtitle: null, root_pid: null, pids: [], ports: [], framework: null, project: null, project_root: null, container: null, tunnel: null, cluster: null, cpu_percent: 0, memory_bytes: 0, is_dev: false, protected: false },
  ];
  const edge = (from: string, to: string, port: number, connections: number, kind: GraphEdge["kind"] = "local", remotes: string[] = []): GraphEdge => ({ id: `${from}->${to}`, from, to, kind, port, connections, remotes });
  const edges = [
    edge("web", "api", 3001, 4),
    edge("api", "db", 5432, 6),
    edge("api", "redis", 6379, 2),
    edge("docs", "api", 3001, 1),
    edge("ml", "db", 5432, 1),
    edge("ml", "external", 443, 2, "outbound", ["api.openai.com:443", "huggingface.co:443"]),
  ];
  const clusters: Cluster[] = [{ id: "shop", name: "shop", kind: "compose", detail: "docker compose", root: "/Users/dev/code/shop", nodes: ["web", "api", "db", "redis"] }];
  return { nodes, edges, clusters, stats: { nodes: nodes.length, edges: edges.length, clusters: 1, connections: 16 }, taken_at_ms: s.taken_at_ms };
}
