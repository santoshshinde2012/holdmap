import type { ActionPlan, Category, HttpInfo, PortEntry, Step } from "./types";

export type Group = "dev" | "containers" | "data" | "apps" | "system";

export const GROUPS: { id: Group; title: string; hint: string }[] = [
  { id: "dev", title: "Dev servers", hint: "Your running projects" },
  { id: "containers", title: "Containers", hint: "Docker, OrbStack, Podman…" },
  { id: "data", title: "Databases & queues", hint: "Postgres, Redis, Kafka…" },
  { id: "apps", title: "Apps & tools", hint: "Desktop apps, helpers, tools" },
  { id: "system", title: "System & other users", hint: "Usually leave these alone" },
];

const DATA: Category[] = ["database", "cache", "queue"];

export function groupOf(e: PortEntry): Group {
  if (e.container) return "containers";
  const cat = e.framework?.category;
  if (!e.process || e.protected || cat === "system" || (!e.is_mine && e.pid === null)) return "system";
  if (cat && DATA.includes(cat)) return "data";
  if (e.is_dev) return "dev";
  if (!e.is_mine) return "system";
  return "apps";
}

/** Hue (OKLCH) for a framework badge so Next/Vite/Django etc. are recognisable at a glance. */
export function tone(e: PortEntry): string {
  const g = groupOf(e);
  if (g === "containers") return "blue";
  if (g === "data") return "violet";
  if (g === "system") return "gray";
  if (g === "dev") return "green";
  return "amber";
}

export function title(e: PortEntry): string {
  if (e.container) return e.container.compose_service ?? e.container.name;
  if (e.project) return e.project.name;
  if (e.framework) return e.framework.name;
  if (e.process) return e.process.name;
  return e.user ? `owned by ${e.user}` : "unknown owner";
}

export function subtitle(e: PortEntry): string {
  const bits: string[] = [];
  if (e.container) bits.push(e.container.image);
  else if (e.process) bits.push(`${e.process.name} · PID ${e.process.pid}`);
  else if (e.pid) bits.push(`PID ${e.pid}`);
  else bits.push("process hidden — needs elevation");
  return bits.join(" · ");
}

export function command(e: PortEntry): string {
  return e.process?.cmdline.join(" ") ?? "";
}

export function humanDuration(secs: number): string {
  if (secs < 0) secs = 0;
  if (secs < 60) return `${Math.floor(secs)}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h`;
}

/** One line for an HTTP probe: `200 OK · “Acme Shop”`, `302 Found → /login`. */
export function httpSummary(h: HttpInfo): string {
  let s = `${h.status} ${h.reason}`.trim();
  if (h.title) s += ` · “${h.title}”`;
  if (h.location) s += ` → ${h.location}`;
  if (h.server) s += ` · ${h.server}`;
  return s;
}

export function uptime(e: PortEntry, nowMs = Date.now()): string | null {
  if (!e.process?.start_time) return null;
  return humanDuration(nowMs / 1000 - e.process.start_time);
}

export function humanBytes(n: number): string {
  if (!n) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function tildify(path: string, home?: string): string {
  if (home && path.startsWith(home)) return "~" + path.slice(home.length);
  return path.replace(/^\/(Users|home)\/[^/]+/, "~");
}

export function seconds(ms: number): string {
  return ms % 1000 === 0 ? `${ms / 1000}s` : `${(ms / 1000).toFixed(1)}s`;
}

export function describeStep(s: Step): string {
  switch (s.action) {
    case "signal_processes": {
      const who = s.processes.map((p) => `${p.name} (${p.pid})`).join(", ");
      return s.force
        ? `Force-kill ${who}`
        : `Send SIGTERM to ${who}; SIGKILL anything still running after ${seconds(s.timeout_ms)}`;
    }
    case "stop_container":
      return `Stop ${s.runtime} container “${s.name}” (graceful, ${s.timeout_s}s)`;
    case "run_command":
      return `Run \`${[s.program, ...s.args].join(" ")}\` — ${s.reason}`;
    case "verify_free":
      return `Check that ${s.protocol.toUpperCase()} port ${s.port} is free (up to ${seconds(s.timeout_ms)})`;
  }
}

export function isForce(plan: ActionPlan): boolean {
  return plan.steps.some((s) => s.action === "signal_processes" && s.force);
}

export function stopTarget(e: PortEntry): string {
  return e.protocol === "udp" ? `${e.port}/udp` : String(e.port);
}

export function url(e: PortEntry): string {
  return `http://localhost:${e.port}`;
}

export function canOpen(e: PortEntry): boolean {
  return e.protocol === "tcp" && e.state === "listen" && (e.is_dev || groupOf(e) === "apps" || !!e.container);
}

export interface Filters {
  query: string;
  all: boolean;
  proto: "any" | "tcp" | "udp";
  dev: boolean;
  mine: boolean;
  exposed: boolean;
}

/** Client-side filter. Supports `:3000`, `3000-3999`, `proto:udp`, `pid:123` and free text. */
export function matches(e: PortEntry, f: Filters): boolean {
  if (f.proto !== "any" && e.protocol !== f.proto) return false;
  if (f.dev && !e.is_dev) return false;
  if (f.mine && !e.is_mine) return false;
  if (f.exposed && e.exposure !== "all_interfaces") return false;
  for (const raw of f.query.trim().toLowerCase().split(/\s+/).filter(Boolean)) {
    const port = raw.match(/^:?(\d{1,5})$/);
    if (port) {
      if (raw.startsWith(":") ? e.port !== +port[1] : !String(e.port).startsWith(port[1])) return false;
      continue;
    }
    const range = raw.match(/^(\d{1,5})-(\d{1,5})$/);
    if (range) {
      if (e.port < +range[1] || e.port > +range[2]) return false;
      continue;
    }
    if (raw.startsWith("proto:")) {
      if (e.protocol !== raw.slice(6)) return false;
      continue;
    }
    if (raw.startsWith("pid:")) {
      if (!e.pids.includes(+raw.slice(4))) return false;
      continue;
    }
    const hay = [
      e.label,
      e.process?.name,
      command(e),
      e.project?.name,
      e.project?.git_branch,
      e.project?.root,
      e.process?.cwd,
      e.framework?.name,
      e.container?.name,
      e.container?.image,
      e.user,
      e.addresses.join(" "),
    ]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    if (!hay.includes(raw)) return false;
  }
  return true;
}

/** True if the user may explicitly override this block ("stop anyway"). Hard protections
 *  (holdmap's own process tree, core OS processes) are never overridable. */
export function canOverride(plan: Pick<ActionPlan, "blocked">): boolean {
  return plan.blocked?.kind === "protected" && plan.blocked.overridable === true;
}
