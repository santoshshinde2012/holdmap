// Pure view logic for the Agents view: the footprint graph (agents in the centre column, the
// folders they work in on the left, the ports they run and services they use on the right,
// remote hosts at the far right), its deterministic column layout, conversion to Svelte Flow
// nodes and edges with self-computed edge geometry, and the summaries the agent cards show.
// No DOM or Svelte imports, so all of it is unit-tested.

import type { AccessFact, AccessLevel, AccessTopic, Agent, AgentKind, AgentPort, PortEntry, AgentProcess, AgentToolKind, AgentsReport, Evidence } from "./types";

export type FootKind = "agent" | "folder" | "tool" | "port" | "service" | "remote" | "more" | "omitted" | "header";
export type FootEdgeKind = "parent" | "folder" | "recent" | "tool" | "runs" | "uses" | "remote" | "omitted";
export type Tone = "warn" | "muted" | null;

export interface FootNode {
  id: string;
  kind: FootKind;
  label: string;
  sub: string;
  /** Agents connected to this node (ids), in report order. */
  owners: string[];
  /** Port entry behind a port/service node: opens the details pane. */
  entryId: string | null;
  /** Folder path, for folder nodes. */
  path: string | null;
  evidence: Evidence;
  tone: Tone;
  warning?: string;
  /** The agent itself, for agent nodes. */
  agent: Agent | null;
}

export interface FootEdge {
  id: string;
  from: string;
  to: string;
  kind: FootEdgeKind;
  label: string | null;
  inferred: boolean;
  connections: number;
}

export interface FootGraph {
  nodes: FootNode[];
  edges: FootEdge[];
}

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

/** Per-agent accent, cycled: edges and badges share it so each agent's footprint reads as a group. */
export const AGENT_COLORS = ["var(--accent)", "var(--tone-amber)", "var(--tone-green)", "var(--tone-violet)", "var(--tone-blue)"];

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

export interface GraphOptions {
  /** Remote hosts shown per agent; the rest fold into a "+N hosts" node. */
  remoteLimit?: number;
  /** Include recent (not currently open) projects. */
  recent?: boolean;
}

/** The footprint graph of a report. Nodes shared by several agents (a folder two agents work in,
 *  one agent's dev server another agent uses) appear once, linked to each. */
export function footprintGraph(r: AgentsReport, opts: GraphOptions = {}): FootGraph {
  const remoteLimit = opts.remoteLimit ?? 3;
  const recent = opts.recent ?? true;
  const nodes = new Map<string, FootNode>();
  const edges = new Map<string, FootEdge>();
  const ids = new Set(r.agents.map((a) => a.id));
  const node = (n: Omit<FootNode, "owners">, owner: string) => {
    const cur = nodes.get(n.id);
    if (cur) {
      if (!cur.owners.includes(owner)) cur.owners.push(owner);
      // A folder that one agent works in and another only lists as recent is observed.
      if (cur.evidence !== "observed" && n.evidence === "observed") cur.evidence = "observed";
      // …and a working folder for anyone is not just "recent".
      if (cur.tone === "muted" && n.tone === null && n.kind === "folder") Object.assign(cur, { tone: null, sub: n.sub });
      return cur;
    }
    const created = { ...n, owners: [owner] };
    nodes.set(n.id, created);
    return created;
  };
  const edge = (e: Omit<FootEdge, "id">) => {
    const id = `${e.from}->${e.to}:${e.kind}`;
    if (!edges.has(id)) edges.set(id, { ...e, id });
  };
  const base = { entryId: null, path: null, evidence: "observed" as Evidence, tone: null as Tone, agent: null };

  for (const a of r.agents) {
    const mem = a.memory_bytes >= 1 << 30 ? `${(a.memory_bytes / (1 << 30)).toFixed(1)} GB` : `${Math.round(a.memory_bytes / (1 << 20))} MB`;
    node({ ...base, id: a.id, kind: "agent", label: a.name, sub: `${KIND_LABEL[a.kind]} · pid ${a.pid} · ${mem}`, agent: a }, a.id);
  }
  // Ports first, so a dev server that another agent uses stays a "port" node.
  for (const a of r.agents) {
    for (const p of a.ports) {
      const id = `port:${p.entry_id}`;
      node({ ...base, id, kind: "port", label: p.project ?? p.framework ?? p.process ?? shortLabel(p.label), sub: `:${p.port}${p.framework && p.project ? ` · ${p.framework}` : ""}`, entryId: p.entry_id, tone: p.exposure !== "loopback" ? "warn" : null, warning: p.exposure === "specific" ? "Bound to a specific interface" : p.exposure === "all_interfaces" ? "Reachable from the network" : undefined }, a.id);
      edge({ from: a.id, to: id, kind: "runs", label: null, inferred: false, connections: 0 });
    }
  }
  for (const a of r.agents) {
    if (a.parent && ids.has(a.parent)) edge({ from: a.parent, to: a.id, kind: "parent", label: "started", inferred: false, connections: 0 });
    for (const t of a.tools ?? []) {
      // MCP can use stdio and have no listening socket, so it needs its own node.
      // Shells and commands stay in the card; dev servers already have port nodes.
      if (t.kind !== "mcp_server") continue;
      const id = `tool:${t.pid}`;
      node({ ...base, id, kind: "tool", label: t.name, sub: `MCP · pid ${t.pid}${t.ports.length ? ` · :${t.ports.join(", :")}` : " · no listener"}`, evidence: t.evidence }, a.id);
      edge({ from: a.id, to: id, kind: "tool", label: null, inferred: t.evidence !== "observed", connections: 0 });
    }
    for (const f of a.folders) {
      if (f.source === "recent" && !recent) continue;
      const id = `folder:${f.path}`;
      node({ ...base, id, kind: "folder", label: f.label, sub: f.project?.git_branch ? `⎇ ${f.project.git_branch}` : f.source === "recent" ? "recent project" : "working folder", path: f.path, evidence: f.evidence, tone: f.source === "recent" ? "muted" : null }, a.id);
      edge({ from: a.id, to: id, kind: f.source === "recent" ? "recent" : "folder", label: null, inferred: f.evidence !== "observed", connections: 0 });
    }
    const locals = a.links.filter((l) => l.kind === "local");
    for (const l of locals) {
      const id = l.entry_id ? `port:${l.entry_id}` : `svc:${l.id}`;
      node({ ...base, id, kind: "service", label: shortLabel(l.label), sub: `:${l.port}${l.process ? ` · ${l.process}` : ""}`, entryId: l.entry_id }, a.id);
      edge({ from: a.id, to: id, kind: "uses", label: `×${l.connections}`, inferred: false, connections: l.connections });
    }
    const remotes = a.links.filter((l) => l.kind === "remote");
    for (const l of remotes.slice(0, remoteLimit)) {
      const id = `remote:${l.address}`;
      node({ ...base, id, kind: "remote", label: l.address.replace(/:\d+$/, ""), sub: l.service ?? `:${l.port}` }, a.id);
      edge({ from: a.id, to: id, kind: "remote", label: `×${l.connections}`, inferred: false, connections: l.connections });
    }
    const rest = remotes.slice(remoteLimit);
    if (rest.length > 0) {
      const id = `more:${a.id}`;
      const conns = rest.reduce((n, l) => n + l.connections, 0);
      node({ ...base, id, kind: "more", label: `+${rest.length} more remote links`, sub: `${conns} connection${conns === 1 ? "" : "s"}`, tone: "muted" }, a.id);
      edge({ from: a.id, to: id, kind: "remote", label: null, inferred: false, connections: conns });
    }
    if (a.more_links > 0) {
      const id = `omitted:${a.id}`;
      node({ ...base, id, kind: "omitted", label: `+${a.more_links} unlisted links`, sub: "Local or remote", tone: "muted" }, a.id);
      edge({ from: a.id, to: id, kind: "omitted", label: null, inferred: false, connections: 0 });
    }
  }
  return { nodes: [...nodes.values()], edges: [...edges.values()] };
}

// ---- layout ----

export const AGENT_W = 248;
export const AGENT_H = 84;
export const FOOT_W = 216;
export const FOOT_H = 56;
export const COL_GAP = 112;
export const ROW_GAP = 18;
export const HEADER_Y_GAP = 54;

export type Point = { x: number; y: number };
export type Positions = Map<string, Point>;

/** Which column a node sits in: folders, agents, ports & services, remote hosts. */
export function column(kind: FootKind): number {
  return kind === "folder" ? 0 : kind === "agent" ? 1 : kind === "port" || kind === "service" || kind === "tool" || kind === "omitted" ? 2 : 3;
}

export const COLUMN_TITLES = ["Folders", "Agents", "Tools, ports & services", "Remote hosts"];

export const sizeOf = (kind: FootKind) => (kind === "agent" ? { w: AGENT_W, h: AGENT_H } : kind === "header" ? { w: FOOT_W, h: 20 } : { w: FOOT_W, h: FOOT_H });

/** Column centre x positions. */
export function columnX(): number[] {
  const widths = [FOOT_W, AGENT_W, FOOT_W, FOOT_W];
  const xs = [widths[0] / 2];
  for (let c = 1; c < widths.length; c++) xs.push(xs[c - 1] + widths[c - 1] / 2 + COL_GAP + widths[c] / 2);
  return xs;
}

/** Stack items (with wished-for centre y) in order, at least `gap` apart, then centre the stack
 *  on its wishes so a column doesn't drift. */
function stack(wishes: { id: string; y: number; h: number }[], gap: number): Map<string, number> {
  const out = new Map<string, number>();
  let prev = -Infinity;
  const placed = wishes.map((w) => {
    const top = Math.max(w.y - w.h / 2, prev);
    prev = top + w.h + gap;
    return { id: w.id, y: top + w.h / 2 };
  });
  const shift = placed.length ? wishes.reduce((s, w, i) => s + (w.y - placed[i].y), 0) / placed.length : 0;
  for (const p of placed) out.set(p.id, p.y + shift);
  return out;
}

/** Deterministic column layout: agents in report order (an agent right after the one it was
 *  started from), every other column ordered by the mean position of its agents (fewer
 *  crossings), then agents re-centred on their footprint. Returns centre points. */
export function layoutFootprint(g: FootGraph): Positions {
  const xs = columnX();
  const agents = g.nodes.filter((n) => n.kind === "agent");
  // Parents first, children right after them.
  const order: FootNode[] = [];
  const childrenOf = (id: string) => agents.filter((a) => a.agent?.parent === id);
  const visit = (a: FootNode) => {
    if (order.includes(a)) return;
    order.push(a);
    for (const c of childrenOf(a.id)) visit(c);
  };
  for (const a of agents) if (!a.agent?.parent || !agents.some((x) => x.id === a.agent!.parent)) visit(a);
  for (const a of agents) visit(a);

  const pos: Positions = new Map();
  const agentY = new Map<string, number>();
  let y = 0;
  for (const a of order) {
    agentY.set(a.id, y + AGENT_H / 2);
    y += AGENT_H + ROW_GAP * 4;
  }
  const others = g.nodes.filter((n) => n.kind !== "agent");
  const byCol = [0, 2, 3].map((c) => others.filter((n) => column(n.kind) === c));
  const place = () => {
    for (const [i, col] of byCol.entries()) {
      const c = [0, 2, 3][i];
      const wishes = col
        .map((n) => ({ id: n.id, y: n.owners.reduce((s, o) => s + (agentY.get(o) ?? 0), 0) / Math.max(1, n.owners.length), h: FOOT_H, kind: n.kind, label: n.label }))
        .sort((a, b) => a.y - b.y || (a.kind === b.kind ? 0 : a.kind === "port" ? -1 : 1) || a.label.localeCompare(b.label));
      const ys = stack(wishes, ROW_GAP);
      for (const w of wishes) pos.set(w.id, { x: xs[c], y: ys.get(w.id)! });
    }
  };
  place();
  // Re-centre each agent on its footprint, keeping the agent order and spacing.
  const wishes = order.map((a) => {
    const mine = others.filter((n) => n.owners.includes(a.id)).map((n) => pos.get(n.id)!.y);
    return { id: a.id, y: mine.length ? mine.reduce((s, v) => s + v, 0) / mine.length : agentY.get(a.id)!, h: AGENT_H };
  });
  const ys = stack(wishes, ROW_GAP * 3);
  for (const a of order) agentY.set(a.id, ys.get(a.id)!);
  place();
  for (const a of order) pos.set(a.id, { x: xs[1], y: agentY.get(a.id)! });
  return pos;
}

/** Column header positions: centred over each non-empty column, above its top node. */
export function headers(g: FootGraph, pos: Positions): { id: string; label: string; x: number; y: number }[] {
  const xs = columnX();
  const top = Math.min(...[...pos.values()].map((p) => p.y)) - AGENT_H / 2 - HEADER_Y_GAP / 2;
  return COLUMN_TITLES.map((label, c) => ({ id: `header:${c}`, label, x: xs[c], y: top, used: g.nodes.some((n) => column(n.kind) === c) }))
    .filter((h) => h.used)
    .map(({ id, label, x, y }) => ({ id, label, x, y }));
}

// ---- focus ----

/** A node plus its direct neighbours and the edges between them (hover / selection highlight). */
export function neighbours(g: FootGraph, id: string): { nodes: Set<string>; edges: Set<string> } {
  const nodes = new Set([id]);
  const edges = new Set<string>();
  for (const e of g.edges) {
    if (e.from === id || e.to === id) {
      edges.add(e.id);
      nodes.add(e.from);
      nodes.add(e.to);
    }
  }
  return { nodes, edges };
}

// ---- Svelte Flow conversion ----

export interface FootFlowData extends Record<string, unknown> {
  node: FootNode;
  color: string;
  /** One colour per agent touching the node (shared folders and services). */
  ownerColors: string[];
  dim: boolean;
  hl: boolean;
  selected: boolean;
}

export interface FootFlowNode {
  id: string;
  type: "agent" | "foot" | "header";
  position: Point;
  data: FootFlowData;
  width: number;
  height: number;
  selectable?: boolean;
  focusable?: boolean;
  draggable?: boolean;
}

export interface EdgeGeometry {
  path: string;
  labelX: number;
  labelY: number;
}

export interface FootFlowEdgeData extends Record<string, unknown> {
  edge: FootEdge;
  geometry: EdgeGeometry;
  color: string;
  dim: boolean;
  hl: boolean;
}

export interface FootFlowEdge {
  id: string;
  source: string;
  target: string;
  type: "foot";
  data: FootFlowEdgeData;
}

/** Edge path between two node boxes (centre + size): side to side with a horizontal bezier, or,
 *  for two agents in the same column, a bracket that bulges out to the left. */
export function edgeGeometry(a: Point & { w: number; h: number }, b: Point & { w: number; h: number }): EdgeGeometry {
  if (Math.abs(a.x - b.x) < 1) {
    const x = a.x - a.w / 2;
    const bulge = Math.min(COL_GAP * 0.42, 44);
    const path = `M ${x} ${a.y} C ${x - bulge} ${a.y}, ${x - bulge} ${b.y}, ${b.x - b.w / 2} ${b.y}`;
    return { path, labelX: x - bulge * 0.75, labelY: (a.y + b.y) / 2 };
  }
  const right = b.x > a.x;
  const sx = right ? a.x + a.w / 2 : a.x - a.w / 2;
  const tx = right ? b.x - b.w / 2 : b.x + b.w / 2;
  const dx = (tx - sx) / 2;
  const path = `M ${sx} ${a.y} C ${sx + dx} ${a.y}, ${tx - dx} ${b.y}, ${tx} ${b.y}`;
  // Label three quarters of the way along, near its target: clear of the other edges leaving
  // the same agent, and apart from other agents' labels on a shared target.
  const t = 0.75, u = 1 - t;
  const bez = (p0: number, p1: number, p2: number, p3: number) => u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
  return { path, labelX: Math.round(bez(sx, sx + dx, tx - dx, tx)), labelY: Math.round(bez(a.y, a.y, b.y, b.y)) };
}

export interface FootFlowOptions {
  focus?: string | null;
  selectedId?: string | null;
  /** Full report order keeps agent colours stable when search hides other agents. */
  agentIds?: readonly string[];
}

/** The agent's colour, by its position in the report. */
export function agentColor(g: FootGraph, agentId: string, agentIds?: readonly string[]): string {
  const i = (agentIds ?? g.nodes.filter((n) => n.kind === "agent").map((n) => n.id)).indexOf(agentId);
  return AGENT_COLORS[(i < 0 ? 0 : i) % AGENT_COLORS.length];
}

export function toFootFlow(g: FootGraph, pos: Positions, opts: FootFlowOptions = {}): { nodes: FootFlowNode[]; edges: FootFlowEdge[] } {
  const rel = opts.focus ? neighbours(g, opts.focus) : null;
  const byId = new Map(g.nodes.map((n) => [n.id, n]));
  const nodes: FootFlowNode[] = [];
  for (const h of headers(g, pos)) {
    const { w, h: hh } = sizeOf("header");
    const n: FootNode = { id: h.id, kind: "header", label: h.label, sub: "", owners: [], entryId: null, path: null, evidence: "observed", tone: null, agent: null };
    nodes.push({ id: h.id, type: "header", position: { x: h.x - w / 2, y: h.y - hh / 2 }, width: w, height: hh, selectable: false, focusable: false, draggable: false, data: { node: n, color: "", ownerColors: [], dim: false, hl: false, selected: false } });
  }
  for (const n of g.nodes) {
    const p = pos.get(n.id);
    if (!p) continue;
    const { w, h } = sizeOf(n.kind);
    nodes.push({
      id: n.id,
      type: n.kind === "agent" ? "agent" : "foot",
      position: { x: p.x - w / 2, y: p.y - h / 2 },
      width: w,
      height: h,
      draggable: false,
      data: { node: n, color: agentColor(g, n.kind === "agent" ? n.id : n.owners[0], opts.agentIds), ownerColors: n.owners.map((o) => agentColor(g, o, opts.agentIds)), dim: !!rel && !rel.nodes.has(n.id), hl: !!rel && rel.nodes.has(n.id) && n.id !== opts.focus, selected: opts.selectedId === n.id },
    });
  }
  const edges: FootFlowEdge[] = [];
  for (const e of g.edges) {
    const a = pos.get(e.from), b = pos.get(e.to);
    const na = byId.get(e.from), nb = byId.get(e.to);
    if (!a || !b || !na || !nb) continue;
    const geometry = edgeGeometry({ ...a, ...sizeOf(na.kind) }, { ...b, ...sizeOf(nb.kind) });
    edges.push({ id: e.id, source: e.from, target: e.to, type: "foot", data: { edge: e, geometry, color: agentColor(g, e.kind === "parent" ? e.to : e.from, opts.agentIds), dim: !!rel && !rel.edges.has(e.id), hl: !!rel && rel.edges.has(e.id) } });
  }
  return { nodes, edges };
}

/** Structure key: re-layout only when this changes, so live refreshes don't move nodes. */
export function signature(g: FootGraph): string {
  return `${g.nodes.map((n) => `${n.id}@${n.owners.join("+")}`).join(",")}|${g.edges.map((e) => e.id).join(",")}`;
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

/** "1 folder · 2 ports · 3 links · 1 stoppable". */
export function countsLine(c: Counts): string {
  const p = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;
  const parts = [p(c.folders, "folder"), p(c.ports, "port"), p(c.links, "link")];
  if (c.stoppable > 0) parts.push(p(c.stoppable, "stoppable", "stoppable"));
  return parts.join(" · ");
}

/** Compact resource line for skim surfaces: "520 MB · 3.4% · 15 processes". */
export function resourcesLine(a: Agent): string {
  const procs = counts(a).processes;
  const cpu = a.cpu_percent > 0 ? `${a.cpu_percent < 10 ? a.cpu_percent.toFixed(1) : Math.round(a.cpu_percent)}%` : null;
  // Match CLI human_bytes style roughly for the card (KB/MB/GB).
  const mem = a.memory_bytes >= 1 << 30
    ? `${(a.memory_bytes / (1 << 30)).toFixed(a.memory_bytes >= 10 * (1 << 30) ? 0 : 1)} GB`
    : a.memory_bytes >= 1 << 20
      ? `${Math.round(a.memory_bytes / (1 << 20))} MB`
      : a.memory_bytes >= 1 << 10
        ? `${Math.round(a.memory_bytes / (1 << 10))} KB`
        : `${a.memory_bytes} B`;
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

/** Monogram for an agent tile: "Claude Code" → "CC", "Cursor" → "Cu". */
export function monogram(name: string): string {
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
