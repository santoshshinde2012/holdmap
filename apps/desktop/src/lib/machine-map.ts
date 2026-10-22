// Machine map view model: each agent's footprint (observed project folders, listening ports,
// configured MCP servers, remote hosts) and a radial layout that fans it out in the agent's sector.
import { isUnpinned } from "./agent-config";
import type { Agent } from "./types";

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
  target: { view: "list"; entryId: string } | { view: "agents"; agentId: string } | { view: "expand"; groupId: string };
  /** Full text when `label` was shortened; shown as the tooltip. */
  full?: string;
  /** A remote-host group toggle: how many hosts it stands for. */
  count?: number;
  /** The toggle of an expanded group (hides the hosts again); counts for nothing itself. */
  collapse?: boolean;
}

/** Longest label drawn on the map; longer ones are shortened with an ellipsis and a tooltip. */
export const MAP_LABEL_MAX = 24;

/** Shortens `text` to `max` characters with an ellipsis in the middle, so both ends stay readable. */
export function shorten(text: string, max = MAP_LABEL_MAX): string {
  if (text.length <= max) return text;
  const head = Math.ceil((max - 1) * 0.6);
  return text.slice(0, head) + "…" + text.slice(text.length - (max - 1 - head));
}

function short(node: FootprintNode): FootprintNode {
  const label = shorten(node.label);
  const sub = node.sub === undefined ? undefined : shorten(node.sub);
  if (label === node.label && sub === node.sub) return node;
  return { ...node, label, sub, full: node.sub ? `${node.label} · ${node.sub}` : node.label };
}

/** Id of an agent's collapsed remote-host group. */
export const remoteGroupId = (agentId: string) => `${agentId}:remotes`;

/** Everything one agent reaches, in a fixed kind order: projects, ports, MCP servers, remote hosts.
 *  Two or more remote hosts collapse into one count node unless `expanded` holds the agent's
 *  group id, so a chatty agent doesn't bury its own projects and ports. */
export function footprint(agent: Agent, max = 14, expanded: ReadonlySet<string> = new Set()): FootprintNode[] {
  const toAgent = { view: "agents" as const, agentId: agent.id };
  const out: FootprintNode[] = [];
  const projects = [...new Map(agent.folders.filter((f) => f.source !== "recent").map((f) => [f.project?.name ?? f.label, f])).keys()];
  for (const p of projects) out.push({ id: `${agent.id}:p:${p}`, kind: "project", label: p, warn: false, live: true, target: toAgent });
  for (const p of agent.ports) out.push({ id: `${agent.id}:port:${p.entry_id}`, kind: "port", label: `:${p.port}`, sub: p.framework ?? p.label, warn: p.exposure === "all_interfaces", live: true, target: { view: "list", entryId: p.entry_id } });
  for (const s of agent.capabilities?.mcp_servers ?? []) {
    if (s.enabled === false) continue;
    out.push({ id: `${agent.id}:mcp:${s.id}`, kind: "mcp", label: s.name, sub: s.remote_host ?? s.launch?.runner, warn: isUnpinned(s), live: s.runtime?.state === "running", target: toAgent });
  }
  const remotes = agent.links.filter((l) => l.kind === "remote");
  const group = remoteGroupId(agent.id);
  if (remotes.length > 1 && !expanded.has(group)) {
    const services = [...new Set(remotes.map((l) => l.service).filter((v): v is string => !!v))];
    out.push({ id: group, kind: "remote", label: `${remotes.length} remote hosts`, sub: services.slice(0, 2).join(", ") || undefined, warn: false, live: true, count: remotes.length, target: { view: "expand", groupId: group } });
  } else {
    if (remotes.length > 1) out.push({ id: group, kind: "remote", label: "Hide remote hosts", warn: false, live: true, count: remotes.length, collapse: true, target: { view: "expand", groupId: group } });
    for (const l of remotes) out.push({ id: `${agent.id}:r:${l.id}`, kind: "remote", label: l.service ?? l.address, sub: l.service ? l.address : `${l.connections} conn`, warn: false, live: true, target: toAgent });
  }
  return out.slice(0, max).map(short);
}

export interface Placed<T> { item: T; x: number; y: number; angle: number }

/** Elliptical stretch so the map fills a wide stage. */
export const MAP_STRETCH = { x: 1.45, y: 0.75 };
/** Ring radii before the stretch: agents on the inner ring, footprints on the outer. */
export const MAP_RINGS = { inner: 165, outer: 300 };

/** Radial layout: agents on an inner ring, each agent's footprint fanned out in its own sector. */
export function radialLayout(agents: Agent[], inner = MAP_RINGS.inner, outer = MAP_RINGS.outer, max = 14, stretch = MAP_STRETCH, expanded: ReadonlySet<string> = new Set()) {
  const n = Math.max(1, agents.length);
  const sector = (Math.PI * 2) / n;
  return agents.map((agent, i) => {
    const angle = -Math.PI / 2 + i * sector;
    const nodes = footprint(agent, max, expanded);
    const spread = Math.min(sector * 0.86, nodes.length * 0.3);
    const step = nodes.length > 1 ? spread / (nodes.length - 1) : 0;
    const placed: Placed<FootprintNode>[] = nodes.map((item, j) => {
      const a = angle - spread / 2 + j * step;
      const r = outer + (nodes.length > 5 && j % 2 ? 62 : 0);
      return { item, angle: a, x: Math.cos(a) * r * stretch.x, y: Math.sin(a) * r * stretch.y };
    });
    return { agent, angle, x: Math.cos(angle) * inner * stretch.x, y: Math.sin(angle) * inner * stretch.y, nodes: placed };
  });
}

/** A node's label box (node marker included), estimated like `mapBounds`. */
export function labelBox(n: Placed<FootprintNode>, k = 1): Box {
  // A count chip is wider than a dot (22 vs 16) and pushes its label out by the difference.
  const dot = n.item.count ? 12 : 8;
  const w = (14 + (dot - 8) + Math.max(n.item.label.length, n.item.sub?.length ?? 0) * CHAR) * k;
  const top = n.y - 10 * k, h = (n.item.sub ? 28 : 16) * k;
  return Math.cos(n.angle) >= -0.01 ? { x: n.x - dot * k, y: top, w: w + dot * k, h } : { x: n.x - w, y: top, w: w + dot * k, h };
}

const overlaps = (a: Box, b: Box, gap: number) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h + gap && b.y < a.y + a.h + gap;

/** Moves footprint nodes apart vertically until no two label boxes overlap (or `rounds` runs
 *  out). Positions change, sides don't, so each label keeps reading away from its agent. */
export function avoidCollisions(groups: ReturnType<typeof radialLayout>, k = 1, gap = 4, rounds = 80): ReturnType<typeof radialLayout> {
  const out = groups.map((g) => ({ ...g, nodes: g.nodes.map((n) => ({ ...n })) }));
  const all = out.flatMap((g) => g.nodes);
  for (let round = 0; round < rounds; round++) {
    let moved = false;
    for (let i = 0; i < all.length; i++) for (let j = i + 1; j < all.length; j++) {
      const a = all[i], b = all[j], ba = labelBox(a, k), bb = labelBox(b, k);
      if (!overlaps(ba, bb, gap)) continue;
      const [upper, lower] = ba.y <= bb.y ? [a, b] : [b, a];
      const [ub, lb] = upper === a ? [ba, bb] : [bb, ba];
      const push = (ub.y + ub.h + gap - lb.y) / 2 + 0.5;
      upper.y -= push; lower.y += push;
      moved = true;
    }
    if (!moved) break;
  }
  return out;
}

/** Whether any two label boxes overlap (tests and debugging). */
export function hasCollisions(groups: ReturnType<typeof radialLayout>, k = 1, gap = 0): boolean {
  const boxes = groups.flatMap((g) => g.nodes.map((n) => labelBox(n, k)));
  return boxes.some((a, i) => boxes.some((b, j) => j > i && overlaps(a, b, gap)));
}

export interface Box { x: number; y: number; w: number; h: number }
const CHAR = 7.2;

/** Fits the view box to what's drawn, labels included (estimated at ~7px per character and scaled
 *  by `k`, the label counter-scale), so edge labels never clip and the map stays centred. */
export function mapBounds(groups: ReturnType<typeof radialLayout>, k = 1, pad = 24): Box {
  let x0 = -110, x1 = 110, y0 = -40, y1 = 40;
  for (const g of groups) {
    const half = (Math.max(g.agent.name.length, 10) * CHAR * k) / 2;
    x0 = Math.min(x0, g.x - half); x1 = Math.max(x1, g.x + half);
    y0 = Math.min(y0, g.y - 66 * k); y1 = Math.max(y1, g.y + 66 * k);
    for (const n of g.nodes) {
      const w = (12 + Math.max(n.item.label.length, n.item.sub?.length ?? 0) * CHAR) * k;
      const right = Math.cos(n.angle) >= -0.01;
      x0 = Math.min(x0, right ? n.x - 16 * k : n.x - w); x1 = Math.max(x1, right ? n.x + w : n.x + 16 * k);
      y0 = Math.min(y0, n.y - 16 * k); y1 = Math.max(y1, n.y + 22 * k);
    }
  }
  return { x: x0 - pad, y: y0 - pad, w: x1 - x0 + pad * 2, h: y1 - y0 + pad * 2 };
}
