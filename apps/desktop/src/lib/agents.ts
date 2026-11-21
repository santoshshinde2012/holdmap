// Facts about an agent that cards, lists, and the tray share: labels, matching, access,
// counts, and colours. The Agents map is agent-picture.ts. No DOM or Svelte imports.

import { cpuReading, humanBytes, humanDuration } from "./format";
import type { AccessFact, AccessLevel, AccessTopic, Agent, AgentKind, AgentPort, PortEntry, AgentProcess, AgentToolKind, AgentsReport, Evidence } from "./types";


export const KIND_LABEL: Record<AgentKind, string> = {
  cli: "Terminal agent",
  ide: "AI editor",
  desktop: "Desktop app",
  extension: "Editor extension",
  host: "Agent host",
  tool: "Developer tool",
};

export const TOPIC_LABEL: Record<AccessTopic, string> = {
  user: "Account",
  sandbox: "Sandbox",
  approvals: "Approvals",
  network: "Network",
  privacy: "Privacy",
};

export const LEVEL_LABEL: Record<AccessLevel, string> = {
  restricted: "Restricted",
  standard: "Standard",
  elevated: "Elevated",
  unknown: "Unknown",
};

export const EVIDENCE_LABEL: Record<Evidence, string> = {
  observed: "seen",
  inferred: "inferred",
  unknown: "unknown",
};

export const TOOL_LABEL: Record<AgentToolKind, string> = { mcp_server: "MCP server", dev_server: "Dev server", shell: "Shell", command: "Command" };

/** Search only identity and footprint metadata; command lines may contain credentials. */
export function agentMatches(a: Agent, query: string): boolean {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return true;
  const pids = new Set([a.pid, ...(a.process_ids ?? []), ...a.processes.map((p) => p.pid)]);
  const text = [a.id, a.name, a.product, a.vendor, a.kind, a.process_name, String(a.pid),
    ...(a.process_ids ?? a.processes.map((p) => p.pid)).map(String),
    ...a.processes.map((p) => p.name),
    ...a.folders.flatMap((f) => [f.path, f.label, f.project?.name ?? ""]),
    ...(a.tools ?? []).flatMap((t) => [t.name, TOOL_LABEL[t.kind], String(t.pid), t.cwd ?? "", ...t.ports.map((p) => `:${p} port:${p}`)]),
    ...(a.capabilities?.inspected ? a.capabilities.mcp_servers.flatMap((server) => [server.name, "MCP server", server.scope, server.source_path, server.project_path ?? ""]) : []),
    ...(a.capabilities?.inspected ? a.capabilities.skills.flatMap((skill) => [skill.name, skill.declared_name ?? "", skill.description ?? "", "skill SKILL.md", skill.scope, skill.source_path, skill.project_path ?? ""]) : []),
    ...a.ports.flatMap((p) => [`:${p.port} port:${p.port}`, p.project ?? "", p.framework ?? "", p.process ?? ""]),
    ...a.links.flatMap((l) => [l.address, `:${l.port} port:${l.port}`, l.service ?? "", l.process ?? ""]),
  ].join(" ").toLocaleLowerCase();
  return terms.every((term) => {
    const pid = /^(?:pid:)?(\d+)$/.exec(term);
    return pid ? pids.has(Number(pid[1])) : text.includes(term);
  });
}

/** Keep report order so searching and keyboard navigation choose the same agents. */
export const filterAgents = (agents: Agent[], query: string): Agent[] => agents.filter((a) => agentMatches(a, query));

/** Nested agent sessions remain discoverable even while the report is filtered. */
export const startedFrom = (report: AgentsReport, parentId: string): Agent[] => report.agents.filter((agent) => agent.parent === parentId);

/** Indent the displayed process tree without inventing missing parents or following cycles. */
export function processRows(a: Agent): { process: AgentProcess; depth: number }[] {
  const byPid = new Map(a.processes.map((p) => [p.pid, p]));
  const children = new Map<number, AgentProcess[]>();
  for (const p of a.processes) if (p.ppid !== null && p.ppid !== p.pid && byPid.has(p.ppid)) {
    const list = children.get(p.ppid) ?? [];
    list.push(p);
    children.set(p.ppid, list);
  }
  const out: { process: AgentProcess; depth: number }[] = [];
  const seen = new Set<number>();
  function visit(p: AgentProcess, depth: number) {
    if (seen.has(p.pid)) return;
    seen.add(p.pid);
    out.push({ process: p, depth: Math.min(depth, 8) });
    for (const child of children.get(p.pid) ?? []) visit(child, depth + 1);
  }
  for (const p of a.processes) if (p.pid === a.pid || p.ppid === null || !byPid.has(p.ppid)) visit(p, 0);
  for (const p of a.processes) visit(p, 0);
  return out;
}

/** Per-agent identity colour. A dedicated palette, so no agent ever looks like an OK, warning or error status.
 *  The same product keeps one colour (two Claude sessions match; Codex does not look like Claude).
 *  Agents with no name, as in tests, still take the next palette slot in order. */
export const AGENT_COLORS = ["var(--agent-1)", "var(--agent-2)", "var(--agent-3)", "var(--agent-4)", "var(--agent-5)"];

/** Known products get fixed slots so a hash cannot paint Claude and Codex the same. */
const PRODUCT_COLOR: Record<string, number> = {
  "Claude Code": 0,
  "Codex CLI": 1,
  "Cursor": 2,
  "Cursor Agent": 3,
  "Docker Desktop": 4,
};

function colorSlot(name: string): number {
  const known = PRODUCT_COLOR[name];
  if (known !== undefined) return known;
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 33 + name.charCodeAt(i)) >>> 0;
  return hash % AGENT_COLORS.length;
}

/** Every agent's colour by id. The same product keeps one colour across cards and the map. */
export function agentColors(agents: readonly { id: string; name?: string }[]): Map<string, string> {
  return new Map(agents.map((a, i) => [a.id, AGENT_COLORS[(a.name ? colorSlot(a.name) : i) % AGENT_COLORS.length]]));
}

/** "PostgreSQL · container shop-db-1" → "PostgreSQL". */
export const shortLabel = (label: string) => label.split(" · ")[0] || label;

/** `/Users/dev/code/shop-web` → `~/code/shop-web` when `home` is known. */
export function tildePath(path: string, home: string | null): string {
  return home && (path === home || path.startsWith(home + "/")) ? "~" + path.slice(home.length) : path;
}

/** The home directory, guessed from the agents' account names and folders (`/Users/dev`). */
export function guessHome(r: AgentsReport): string | null {
  for (const a of r.agents) {
    const u = a.access.user;
    if (!u) continue;
    for (const f of a.folders) {
      for (const base of [`/Users/${u}`, `/home/${u}`]) if (f.path === base || f.path.startsWith(base + "/")) return base;
    }
  }
  return null;
}


// ---- cards ----

export interface Counts {
  folders: number;
  ports: number;
  links: number;
  processes: number;
  /** Unprotected ports the agent started (dev servers / services). */
  stoppable: number;
  tools: number;
}

export function counts(a: Agent): Counts {
  return {
    folders: a.folders.filter((f) => f.source !== "recent").length + (a.more_folders ?? 0),
    ports: a.ports.length,
    links: a.links.length + a.more_links,
    processes: a.process_ids?.length || a.processes.length + a.more_processes,
    tools: (a.tools?.length ?? 0) + (a.more_tools ?? 0),
    stoppable: stoppablePorts(a).length,
  };
}

/** The access facts in a fixed topic order (missing topics read as unknown). */
export function accessFacts(a: Agent): AccessFact[] {
  const order: AccessTopic[] = ["user", "sandbox", "approvals", "network", "privacy"];
  return order.map((t) => a.access.facts.find((f) => f.topic === t) ?? { topic: t, level: "unknown", summary: "Not reported.", evidence: "unknown" });
}

export const ACCESS_LABEL: Record<AccessLevel, string> = { restricted: "Restricted", standard: "Standard", elevated: "Elevated", unknown: "Unknown" };

/** The single most important access headline for a card. */
export function accessHeadline(a: Agent): { level: AccessLevel; text: string } {
  const facts = accessFacts(a);
  const elevated = facts.filter((f) => f.level === "elevated");
  if (a.access.root) return { level: "elevated", text: "Runs as root" };
  if (elevated.length) return { level: "elevated", text: `${elevated.map((f) => TOPIC_LABEL[f.topic]).join(" & ")}: wider than usual` };
  const restricted = facts.find((f) => f.level === "restricted");
  if (restricted) return { level: "restricted", text: restricted.summary };
  if (facts.every((f) => f.level === "unknown")) return { level: "unknown", text: "Access not reported" };
  return { level: "standard", text: a.access.mine ? "Your account's access" : `Runs as ${a.access.user ?? "another user"}` };
}

/** The card's count chips as [number, noun]: folders, ports, links, then processes and tools when asked. */
export function countChips(c: Counts, more = false): [number, string][] {
  const p = (n: number, one: string, many = one + "s"): [number, string] => [n, n === 1 ? one : many];
  return [p(c.folders, "folder"), p(c.ports, "port"), p(c.links, "link"), ...(more ? [p(c.processes, "process", "processes"), p(c.tools, "tool")] : [])];
}

/** "1 folder · 2 ports · 3 links · 1 stoppable". */
export function countsLine(c: Counts): string {
  const parts = countChips(c).map(([n, w]) => `${n} ${w}`);
  if (c.stoppable > 0) parts.push(`${c.stoppable} stoppable`);
  return parts.join(" · ");
}

/** "Agent · Anthropic · pid 51200 · up 2h": who it is, for card and detail headers. */
export function agentMeta(a: Agent, nowMs: number, vendor = true): string {
  return [KIND_LABEL[a.kind], vendor && a.vendor !== "unknown" ? a.vendor : null, `pid ${a.pid}`, a.started_at ? `up ${humanDuration(nowMs / 1000 - a.started_at)}` : null].filter(Boolean).join(" · ");
}

/**
 * What tells an agent apart from others of the same product: its first folder, the agent
 * that started it, or its PID. A folder shared by two sessions of the same name is not
 * enough, so the PID is added (`framesleuth · PID 22475`).
 */
export function agentPlace(a: Agent, all: readonly Agent[]): string | null {
  const sameName = all.filter((b) => b.name === a.name);
  const folder = a.folders[0]?.label;
  if (folder) {
    const shared = sameName.filter((b) => b.folders[0]?.label === folder).length > 1;
    return shared ? `${folder} · PID ${a.pid}` : folder;
  }
  const parent = a.parent ? all.find((b) => b.id === a.parent) : undefined;
  if (parent) return `in ${parent.name}`;
  return sameName.length > 1 ? `PID ${a.pid}` : null;
}

/** Compact resource line for skim surfaces: "520 MB · 3.4% · 15 processes". */
export function resourcesLine(a: Agent): string {
  const procs = counts(a).processes;
  const cpu = a.cpu_percent > 0 && !a.cpu_unknown ? cpuReading(a) : null;
  const mem = humanBytes(a.memory_bytes);
  return [mem, cpu, procs ? `${procs} process${procs === 1 ? "" : "es"}` : null].filter(Boolean).join(" · ");
}

/** Parent agent display name, when this one was started from another. */
export function parentName(r: AgentsReport, a: Agent): string | null {
  if (!a.parent) return null;
  return r.agents.find((x) => x.id === a.parent)?.name ?? null;
}

/** Folder source label for cards (matches CLI wording). */
export function folderSourceLabel(source: Agent["folders"][number]["source"]): string {
  return source === "agent" ? "working dir" : source === "child" ? "child cwd" : "recent";
}

/** Monogram for an agent tile: "Claude Code" → "CC", "Cursor" → "Cu".
 *  Codex CLI would also be "CC" (C from Codex, C from CLI), so it takes "Cx". */
const MONOGRAM: Record<string, string> = { "Codex CLI": "Cx" };

export function monogram(name: string): string {
  const known = MONOGRAM[name];
  if (known) return known;
  const words = name.split(/\s+/).filter(Boolean);
  if (words.length > 1) return (words[0][0] + words[1][0]).toUpperCase();
  return name.slice(0, 2);
}

/** Next/previous agent for keyboard navigation (wraps; starts at the first/last). */
export function step(ids: string[], current: string | null, delta: number): string | null {
  if (!ids.length) return null;
  const i = current ? ids.indexOf(current) : -1;
  if (i < 0) return delta > 0 ? ids[0] : ids[ids.length - 1];
  return ids[(i + delta + ids.length) % ids.length];
}

/** Ports the agent started (dev servers and services), not its own IDE / auth listeners. */
export function stoppablePorts(a: Agent): AgentPort[] {
  return a.ports.filter((p) => p.role === "dev_server" || p.role === "service");
}

/** Live port entries matching an agent's stoppable ports, in report order. */
export function stoppableEntries(a: Agent, entries: PortEntry[]): PortEntry[] {
  const byId = new Map(entries.map((e) => [e.id, e]));
  return stoppablePorts(a).map((p) => byId.get(p.entry_id)).filter((e): e is PortEntry => !!e && !e.protected);
}
