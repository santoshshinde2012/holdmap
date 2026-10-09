// Realistic sample data so the UI can be developed in a plain browser (`npm run dev`) and tested.
import type { AccessFact, ActionPlan, Agent, AgentLink, AgentPort, AgentsReport, BindRisk, Cluster, Explanation, Graph, GraphEdge, GraphNode, HttpInfo, PortDetails, PortEntry, Snapshot, StopReport } from "./types";

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
    commands: [`holdmap stop ${port}`, ...(p ? [`kill -TERM ${p.pid}`] : [])],
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
    return { target: `:${port}`, owners: [{ kind: "hidden", uid: 0, user: "root" }], summary: "", steps: [], blocked: { kind: "needs_elevation", message: `The owner of port ${port} belongs to user root; run \`sudo holdmap stop ${port}\`.` }, warnings: [], risk: "high" };
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
 *  ml-service → external. Built from MOCK_SNAPSHOT so ids line up with the list; services the
 *  demo has stopped drop out, with their edges. */
export function mockTopology(s: Snapshot = MOCK_SNAPSHOT): Graph {
  const byPort = (p: number) => s.entries.find((e) => e.port === p);
  const svc = (port: number, id: string, label: string, cluster: string | null, kind: GraphNode["kind"] = "service"): GraphNode | null => {
    const e = byPort(port);
    if (!e) return null;
    return {
      id, kind: e.container ? "container" : kind, label, subtitle: e.label, root_pid: e.pid, pids: e.pids.length ? e.pids : e.pid ? [e.pid] : [],
      ports: [{ port, protocol: e.protocol, exposure: e.exposure, entry_id: e.id }], framework: e.framework, project: e.project?.name ?? null,
      project_root: e.project?.root ?? null, container: e.container, tunnel: null, cluster, cpu_percent: (port % 7) * 0.9,
      memory_bytes: e.process?.memory_bytes ?? 0, is_dev: e.is_dev, protected: e.protected,
    };
  };
  const nodes = [
    svc(3000, "web", "shop-web", "shop"),
    svc(3001, "api", "shop-api", "shop"),
    svc(5432, "db", "db", "shop"),
    svc(6379, "redis", "redis", "shop"),
    svc(5173, "docs", "docs", null),
    svc(8000, "ml", "ml-service", null),
    { id: "external", kind: "external", label: "External", subtitle: null, root_pid: null, pids: [], ports: [], framework: null, project: null, project_root: null, container: null, tunnel: null, cluster: null, cpu_percent: 0, memory_bytes: 0, is_dev: false, protected: false } as GraphNode,
  ].filter((n): n is GraphNode => n !== null);
  const ids = new Set(nodes.map((n) => n.id));
  const edge = (from: string, to: string, port: number, connections: number, kind: GraphEdge["kind"] = "local", remotes: string[] = []): GraphEdge => ({ id: `${from}->${to}`, from, to, kind, port, connections, remotes });
  const edges = [
    edge("web", "api", 3001, 4),
    edge("api", "db", 5432, 6),
    edge("api", "redis", 6379, 2),
    edge("docs", "api", 3001, 1),
    edge("ml", "db", 5432, 1),
    edge("ml", "external", 443, 2, "outbound", ["api.openai.com:443", "huggingface.co:443"]),
  ].filter((e) => ids.has(e.from) && ids.has(e.to));
  const clusters: Cluster[] = [{ id: "shop", name: "shop", kind: "compose", detail: "docker compose", root: "/Users/dev/code/shop", nodes: ["web", "api", "db", "redis"].filter((id) => ids.has(id)) }];
  return { nodes, edges, clusters, stats: { nodes: nodes.length, edges: edges.length, clusters: 1, connections: edges.reduce((n, e) => n + e.connections, 0) }, taken_at_ms: s.taken_at_ms };
}

/** Browser-preview agents: Cursor running shop-web and docs in its terminal, and Claude Code
 *  (started from that terminal) working on shop-api. Neutral sample projects and documentation
 *  IP ranges; ports the demo has stopped drop out, like the topology. */
export function mockAgents(s: Snapshot = MOCK_SNAPSHOT): AgentsReport {
  const byPort = (p: number) => s.entries.find((e) => e.port === p);
  const port = (p: number, role: AgentPort["role"]): AgentPort[] => {
    const e = byPort(p);
    return e ? [{ entry_id: e.id, port: p, protocol: e.protocol, exposure: e.exposure, pid: e.pid, process: e.process?.name ?? null, label: e.label, role, project: e.project?.name ?? null, framework: e.framework?.name ?? null }] : [];
  };
  const local = (p: number, connections: number, service: string | null = null): AgentLink[] => {
    const e = byPort(p);
    return e ? [{ id: `local:${e.id}`, kind: "local", label: e.label, address: `127.0.0.1:${p}`, port: p, connections, entry_id: e.id, process: e.process?.name ?? null, pid: e.pid, service }] : [];
  };
  const remote = (address: string, connections: number): AgentLink => ({ id: `remote:${address}`, kind: "remote", label: address, address, port: 443, connections, entry_id: null, process: null, pid: null, service: "HTTPS" });
  const fact = (topic: AccessFact["topic"], level: AccessFact["level"], summary: string, evidence: AccessFact["evidence"]): AccessFact => ({ topic, level, summary, evidence });
  const unknownPrivacy = fact("privacy", "unknown", "macOS privacy grants (Full Disk Access, Files and Folders) can't be read without Full Disk Access; holdmap doesn't ask for it.", "unknown");
  const network = (ports: AgentPort[], remotes: number) => {
    const list = ports.map((p) => `:${p.port}`).join(", ");
    const tail = remotes ? ` Talks to ${remotes} remote host${remotes === 1 ? "" : "s"}.` : "";
    return fact("network", "standard", `${ports.length ? `Listens on ${list} on this machine only.` : "No listening sockets."}${tail}`, "observed");
  };
  const shopWeb = byPort(3000)?.project ?? { name: "shop-web", root: "/Users/dev/code/shop-web", kind: "package.json", git_branch: "feat/checkout" };
  const shopApi = byPort(3001)?.project ?? { name: "shop-api", root: "/Users/dev/code/shop-api", kind: "package.json", git_branch: "main" };
  const docs = byPort(5173)?.project ?? { name: "docs", root: "/Users/dev/code/docs", kind: "package.json", git_branch: "main" };
  const started = Math.floor(Date.now() / 1000);

  const cursorPorts = [...port(3000, "dev_server"), ...port(5173, "dev_server")];
  const cursorLinks = [...local(3001, 2), remote("198.51.100.24:443", 4), remote("203.0.113.40:443", 1)];
  const cursor: Agent = {
    id: "agent:52000", product: "cursor", name: "Cursor", vendor: "Anysphere", kind: "ide", pid: 52000, process_name: "Cursor",
    command: "/Applications/Cursor.app/Contents/MacOS/Cursor", started_at: started - 86400, parent: null, memory_bytes: 2.4e9, cpu_percent: 6.2,
    processes: [
      { pid: 52000, ppid: 1, name: "Cursor", command: "/Applications/Cursor.app/Contents/MacOS/Cursor", role: "agent", cwd: "/", memory_bytes: 410e6, cpu_percent: 1.8 },
      { pid: 52011, ppid: 52000, name: "Cursor Helper (Plugin)", command: "Cursor Helper (Plugin) --type=utility", role: "helper", cwd: "/", memory_bytes: 820e6, cpu_percent: 2.1 },
      { pid: 52040, ppid: 52011, name: "zsh", command: "-zsh", role: "child", cwd: shopWeb.root, memory_bytes: 6e6, cpu_percent: 0 },
      { pid: 43000, ppid: 52040, name: "node", command: "node node_modules/.bin/next dev", role: "child", cwd: shopWeb.root, memory_bytes: 412e6, cpu_percent: 1.6 },
      { pid: 52060, ppid: 52011, name: "zsh", command: "-zsh", role: "child", cwd: docs.root, memory_bytes: 6e6, cpu_percent: 0 },
      { pid: 45173, ppid: 52060, name: "node", command: "node node_modules/.bin/vite dev", role: "child", cwd: docs.root, memory_bytes: 180e6, cpu_percent: 0.7 },
    ],
    more_processes: 9,
    folders: [
      { path: shopWeb.root, label: shopWeb.name, project: shopWeb, source: "child", evidence: "observed", pids: [52040, 43000], privacy_area: null, note: null },
      { path: docs.root, label: docs.name, project: docs, source: "child", evidence: "observed", pids: [52060, 45173], privacy_area: null, note: null },
      { path: "/Users/dev/code/design-system", label: "design-system", project: null, source: "recent", evidence: "observed", pids: [], privacy_area: null, note: "open in Cursor" },
    ],
    ports: cursorPorts,
    links: cursorLinks,
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "unknown", "No sandbox seen right now. Its commands may still be sandboxed when they run; agent settings aren't read.", "unknown"),
        fact("approvals", "unknown", "No approval flags on its command line; its own settings decide (not read).", "unknown"),
        network(cursorPorts, 2),
        unknownPrivacy,
      ],
    },
  };

  const claudePorts = port(3001, "dev_server");
  const claudeLinks = [...local(5432, 3, "PostgreSQL"), remote("203.0.113.10:443", 3)];
  const claude: Agent = {
    id: "agent:51200", product: "claude-code", name: "Claude Code", vendor: "Anthropic", kind: "cli", pid: 51200, process_name: "claude",
    command: "claude --permission-mode acceptEdits", started_at: started - 5400, parent: "agent:52000", memory_bytes: 520e6, cpu_percent: 3.4,
    processes: [
      { pid: 51200, ppid: 52040, name: "claude", command: "claude --permission-mode acceptEdits", role: "agent", cwd: shopApi.root, memory_bytes: 240e6, cpu_percent: 2.2 },
      { pid: 51230, ppid: 51200, name: "sandbox-exec", command: "sandbox-exec -p … npm test", role: "child", cwd: shopApi.root, memory_bytes: 3e6, cpu_percent: 0 },
      { pid: 43001, ppid: 51200, name: "node", command: "node server/index.js", role: "child", cwd: shopApi.root, memory_bytes: 96e6, cpu_percent: 0.9 },
    ],
    more_processes: 0,
    folders: [
      { path: shopApi.root, label: shopApi.name, project: shopApi, source: "agent", evidence: "observed", pids: [51200, 51230, 43001], privacy_area: null, note: null },
      { path: shopWeb.root, label: shopWeb.name, project: null, source: "recent", evidence: "observed", pids: [], privacy_area: null, note: "Claude Code project" },
    ],
    ports: claudePorts,
    links: claudeLinks,
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "restricted", "Its commands run inside sandbox-exec (macOS Seatbelt) right now.", "observed"),
        fact("approvals", "standard", "Started with --permission-mode acceptEdits: file edits are accepted without asking; commands still ask.", "observed"),
        network(claudePorts, 1),
        unknownPrivacy,
      ],
    },
  };

  const dockerPorts = [...port(5432, "service"), ...port(6379, "service")];
  const docker: Agent = {
    id: "agent:48000", product: "docker-desktop", name: "Docker Desktop", vendor: "Docker", kind: "tool", pid: 48000, process_name: "Docker Desktop",
    command: "/Applications/Docker.app/Contents/MacOS/Docker Desktop", started_at: started - 172800, parent: null, memory_bytes: 1.1e9, cpu_percent: 2.0,
    processes: [
      { pid: 48000, ppid: 1, name: "Docker Desktop", command: "/Applications/Docker.app/Contents/MacOS/Docker Desktop", role: "agent", cwd: "/", memory_bytes: 180e6, cpu_percent: 0.4 },
      { pid: 48020, ppid: 48000, name: "com.docker.backend", command: "com.docker.backend", role: "helper", cwd: "/", memory_bytes: 420e6, cpu_percent: 1.1 },
    ],
    more_processes: 4,
    folders: [],
    ports: dockerPorts,
    links: [],
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "unknown", "No sandbox seen right now. Its commands may still be sandboxed when they run; agent settings aren't read.", "unknown"),
        fact("approvals", "unknown", "No approval flags on its command line; its own settings decide (not read).", "unknown"),
        network(dockerPorts, 0),
        unknownPrivacy,
      ],
    },
  };

  // A stopped dev server's process is gone from its agent too.
  const gone = new Set([3000, 3001, 5173].filter((p) => !byPort(p)).map((p) => 40000 + p));
  const live = (a: Agent): Agent => ({
    ...a,
    processes: a.processes.filter((p) => !gone.has(p.pid)),
    folders: a.folders.map((f) => ({ ...f, pids: f.pids.filter((p) => !gone.has(p)) })),
  });
  return {
    agents: [claude, cursor, docker].map(live),
    platform: "macos",
    taken_at_ms: s.taken_at_ms,
    limits: [
      "Folders are working directories and the agents' recent-project lists; open files aren't collected.",
      "Remote hosts are shown by IP address; holdmap doesn't look names up.",
      "Chats, settings, tokens and credentials are never read.",
      "macOS privacy grants (TCC) can't be read without Full Disk Access, and folders it guards aren't inspected.",
    ],
  };
}
