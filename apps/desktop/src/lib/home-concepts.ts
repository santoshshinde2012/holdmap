// View models for the Home concept prototypes (dev flag `holdmap.home.concept` = map | lanes | brief).
// Pure functions over the same agents report and scan as the shipped Home; nothing is invented.
import { accessHeadline } from "./agents";
import { attention, type AttentionItem } from "./home";
import { humanDuration } from "./format";
import type { Agent, AgentsReport, PortEntry } from "./types";

export type FootprintKind = "project" | "port" | "mcp" | "remote";
export interface FootprintNode {
  id: string;
  kind: FootprintKind;
  label: string;
  sub?: string;
  /** Port exposed to the network, unpinned MCP server, … */
  warn: boolean;
  /** MCP server observed running, port held by a live process. */
  live: boolean;
  target: { view: "list"; entryId: string } | { view: "agents"; agentId: string };
}

/** Everything one agent reaches, in a fixed kind order: projects, ports, MCP servers, remote hosts. */
export function footprint(agent: Agent, max = 14): FootprintNode[] {
  const toAgent = { view: "agents" as const, agentId: agent.id };
  const out: FootprintNode[] = [];
  const projects = [...new Map(agent.folders.filter((f) => f.source !== "recent").map((f) => [f.project?.name ?? f.label, f])).keys()];
  for (const p of projects) out.push({ id: `${agent.id}:p:${p}`, kind: "project", label: p, warn: false, live: true, target: toAgent });
  for (const p of agent.ports) out.push({ id: `${agent.id}:port:${p.entry_id}`, kind: "port", label: `:${p.port}`, sub: p.framework ?? p.label, warn: p.exposure === "all_interfaces", live: true, target: { view: "list", entryId: p.entry_id } });
  for (const s of agent.capabilities?.mcp_servers ?? []) {
    if (s.enabled === false) continue;
    out.push({ id: `${agent.id}:mcp:${s.id}`, kind: "mcp", label: s.name, sub: s.remote_host ?? s.launch?.runner, warn: s.launch?.pinned === false && ["npx", "uvx", "docker"].includes(s.launch.runner), live: s.runtime?.state === "running", target: toAgent });
  }
  for (const l of agent.links.filter((l) => l.kind === "remote")) out.push({ id: `${agent.id}:r:${l.id}`, kind: "remote", label: l.service ?? l.address, sub: l.service ? l.address : `${l.connections} conn`, warn: false, live: true, target: toAgent });
  return out.slice(0, max);
}

export interface Placed<T> { item: T; x: number; y: number; angle: number }

/** Radial layout: agents on an inner ring, each agent's footprint fanned out in its own sector. */
export function radialLayout(agents: Agent[], inner = 150, outer = 300, max = 14) {
  const n = Math.max(1, agents.length);
  const sector = (Math.PI * 2) / n;
  return agents.map((agent, i) => {
    const angle = -Math.PI / 2 + i * sector;
    const nodes = footprint(agent, max);
    const spread = Math.min(sector * 0.86, nodes.length * 0.3);
    const step = nodes.length > 1 ? spread / (nodes.length - 1) : 0;
    const placed: Placed<FootprintNode>[] = nodes.map((item, j) => {
      const a = angle - spread / 2 + j * step;
      const r = outer + (nodes.length > 5 && j % 2 ? 62 : 0);
      return { item, angle: a, x: Math.cos(a) * r, y: Math.sin(a) * r };
    });
    return { agent, angle, x: Math.cos(angle) * inner, y: Math.sin(angle) * inner, nodes: placed };
  });
}

/** A sentence part; parts with a target render as links. */
export interface BriefPart { text: string; target?: { view: "list"; entryId: string } | { view: "agents"; agentId: string } }

function postureClause(a: Agent): string {
  const h = accessHeadline(a);
  if (a.access.root) return "runs as root";
  if (h.level === "restricted") return "runs sandboxed";
  if (h.level === "elevated") return `has ${h.text.toLowerCase()}`;
  if (h.level === "unknown") return "doesn't report its access";
  return "runs with your account's access";
}

/** "3 agents running. Claude Code holds :3001 in shop-api and runs sandboxed. 2 things need you: …" */
export function briefing(entries: PortEntry[] | null, report: AgentsReport | null, maxAgents = 3): { parts: BriefPart[]; items: AttentionItem[] } {
  if (!report || !entries) return { parts: [{ text: "Taking the first look at this computer…" }], items: [] };
  const parts: BriefPart[] = [];
  const agents = report.agents;
  parts.push({ text: agents.length ? `${agents.length} ${agents.length === 1 ? "agent is" : "agents are"} running. ` : "No agents are running. " });
  for (const a of agents.slice(0, maxAgents)) {
    parts.push({ text: a.name, target: { view: "agents", agentId: a.id } });
    const ports = a.ports.slice(0, 2);
    const project = a.folders.find((f) => f.source !== "recent");
    if (ports.length) {
      parts.push({ text: " holds " });
      ports.forEach((p, i) => { if (i) parts.push({ text: " and " }); parts.push({ text: `:${p.port}`, target: { view: "list", entryId: p.entry_id } }); });
    } else parts.push({ text: " holds no ports" });
    if (project) parts.push({ text: ` in ${project.project?.name ?? project.label}` });
    parts.push({ text: ` and ${postureClause(a)}. ` });
  }
  if (agents.length > maxAgents) parts.push({ text: `${agents.length - maxAgents} more are listed below. ` });
  const items = attention(entries, report);
  if (!items.length) parts.push({ text: "Nothing needs you." });
  else {
    parts.push({ text: items.length === 1 ? "1 thing needs you: " : `${items.length} things need you, first: ` });
    parts.push({ text: items[0].title, target: items[0].target });
    parts.push({ text: "." });
  }
  return { parts, items };
}

export interface LaneMark { id: string; at: number; label: string; warn: boolean; target: { view: "list"; entryId: string }; /** Started before the window; pinned to its left edge. */ before: boolean }
export interface Lane { agent: Agent; start: number; clipped: boolean; marks: LaneMark[]; uptime: string }

/** One lane per agent across [from, now] (ms). Port marks sit at their process start time. */
export function lanes(report: AgentsReport | null, entries: PortEntry[] | null, from: number, now: number): Lane[] {
  const byId = new Map((entries ?? []).map((e) => [e.id, e]));
  return (report?.agents ?? []).map((agent) => {
    const start = agent.started_at * 1000;
    const marks: LaneMark[] = agent.ports.flatMap((p) => {
      const t = byId.get(p.entry_id)?.process?.start_time;
      return t ? [{ id: p.entry_id, at: Math.max(from, t * 1000), before: t * 1000 < from, label: `:${p.port} ${p.framework ?? p.label}`, warn: p.exposure === "all_interfaces", target: { view: "list" as const, entryId: p.entry_id } }] : [];
    }).sort((a, b) => a.at - b.at);
    return { agent, start: Math.max(from, start), clipped: start < from, marks, uptime: humanDuration(Math.max(0, now - start) / 1000) };
  }).sort((a, b) => a.start - b.start);
}
