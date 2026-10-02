// Realistic sample data so the UI can be developed in a plain browser (`npm run dev`) and tested.
import type { ActionPlan, Explanation, PortEntry, Snapshot, StopReport } from "./types";

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
    user: "santosh",
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
    user: "santosh",
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
      process: proc(43000, "node", "node /Users/santosh/code/shop-web/node_modules/.bin/next dev", "/Users/santosh/code/shop-web", 7420, 412e6),
      project: { name: "shop-web", root: "/Users/santosh/code/shop-web", kind: "package.json", git_branch: "feat/checkout" },
      framework: { name: "Next.js", category: "dev_server" },
      label: "Next.js · shop-web (feat/checkout)",
      is_dev: true,
    }),
    entry({
      port: 3001,
      process: proc(43001, "node", "node server/index.js", "/Users/santosh/code/shop-api", 7300, 96e6),
      project: { name: "shop-api", root: "/Users/santosh/code/shop-api", kind: "package.json", git_branch: "main" },
      framework: { name: "Express", category: "app_server" },
      label: "Express · shop-api (main)",
      is_dev: true,
    }),
    entry({
      port: 5173,
      addresses: ["::1"],
      families: ["v6"],
      process: proc(45173, "node", "node node_modules/.bin/vite dev", "/Users/santosh/code/docs", 900, 180e6),
      project: { name: "docs", root: "/Users/santosh/code/docs", kind: "package.json", git_branch: "main" },
      framework: { name: "SvelteKit", category: "dev_server" },
      label: "SvelteKit · docs (main)",
      is_dev: true,
    }),
    entry({
      port: 8000,
      addresses: ["0.0.0.0"],
      families: ["v4"],
      exposure: "all_interfaces",
      process: proc(48000, "Python", "python manage.py runserver 0.0.0.0:8000", "/Users/santosh/code/ml-service", 86400 * 2, 140e6),
      project: { name: "ml-service", root: "/Users/santosh/code/ml-service", kind: "manage.py", git_branch: "exp/embeddings" },
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
      port: 8125,
      protocol: "udp",
      process: proc(48125, "statsd", "node stats.js", "/Users/santosh/code/metrics", 4000, 30e6),
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

export function mockPlan(target: string, force: boolean): ActionPlan {
  const port = parseInt(target, 10);
  const e = MOCK_SNAPSHOT.entries.find((x) => x.port === port)!;
  if (!e?.process) {
    return { target: `:${port}`, owners: [{ kind: "hidden", uid: 0, user: "root" }], summary: "", steps: [], blocked: { kind: "needs_elevation", message: `The owner of port ${port} belongs to user root; run \`sudo portwise stop ${port}\`.` }, warnings: [], risk: "high" };
  }
  if (e.protected) {
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
    risk: e.is_dev ? "low" : "medium",
  };
}

export function mockStop(target: string): StopReport {
  const port = parseInt(target, 10);
  const i = MOCK_SNAPSHOT.entries.findIndex((x) => x.port === port);
  if (i >= 0) MOCK_SNAPSHOT.entries.splice(i, 1);
  return { target: `:${port}`, success: true, freed: true, ports_still_busy: [], signalled: [40000 + port], escalated: false, survivors: [], elapsed_ms: 412, log: ["SIGTERM → npm (42999), node (43000)", "all processes exited after 0.4s", `port ${port} is free`], error: null };
}
