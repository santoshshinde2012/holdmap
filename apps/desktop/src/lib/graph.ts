// Pure topology helpers for the graph view: layout (layered via dagre, or a deterministic
// force simulation), conversion to Svelte Flow nodes/edges with cluster group nodes, and
// dependency highlighting. No DOM or Svelte imports so everything here is unit-tested.

import dagre from "@dagrejs/dagre";
import type { Cluster, ClusterKind, Graph, GraphEdge, GraphNode, PortEntry } from "./types";

export const NODE_W = 236;
export const NODE_H = 72;
export const PAD = 22;
export const HEADER = 34;
export { FIT_PADDING, needsMiniMap } from "./flow";

export type LayoutMode = "layered" | "force";
/** Bound the synchronous quadratic simulation so large graphs stay responsive. */
export const FORCE_NODE_LIMIT = 100;
export const layoutModeFor = (nodeCount: number, requested: LayoutMode): LayoutMode => requested === "force" && nodeCount > FORCE_NODE_LIMIT ? "layered" : requested;
export type Point = { x: number; y: number };
/** Centre positions by node id. */
export type Positions = Map<string, Point>;

export const groupId = (clusterId: string) => `cluster:${clusterId}`;

export const CLUSTER_LABEL: Record<ClusterKind, string> = {
  compose: "Compose",
  kubernetes: "Kubernetes",
  supervisor: "Supervisor",
  workspace: "Workspace",
  git: "Git repo",
};

/** Search only displayed topology metadata, never process command lines. Explicit numeric
 *  selectors distinguish :3000 from :30001 and PID 42 from PID 420. Terms combine with AND. */
export function graphNodeMatches(node: GraphNode, query: string): boolean {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return true;
  const pids = new Set([node.root_pid, ...node.pids].filter((pid): pid is number => pid !== null));
  const ports = new Set(node.ports.map((port) => port.port));
  const text = [
    node.label, node.subtitle, node.id, node.project, node.project_root, node.cluster,
    node.framework?.name, node.framework?.category, node.container?.image, node.container?.name,
    node.container?.compose_project, node.container?.compose_service,
    ...node.ports.flatMap((port) => [String(port.port), port.protocol]), ...[...pids].map(String),
  ].filter(Boolean).join(" ").toLowerCase();
  return terms.every((term) => {
    const explicit = /^(pid|port):(\d+)$/.exec(term);
    const shortPort = /^:(\d+)$/.exec(term);
    if (explicit || shortPort) {
      const value = Number(explicit?.[2] ?? shortPort![1]);
      return Number.isSafeInteger(value) && (explicit?.[1] === "pid" ? pids : ports).has(value);
    }
    return text.includes(term);
  });
}

/** A search result includes matched services and their immediate observed neighbours, so
 *  the graph keeps useful context without treating transitive dependencies as matches. */
export function graphForQuery(graph: Graph | null, query: string): Graph | null {
  if (!graph || !query.trim()) return graph;
  const existing = new Set(graph.nodes.map((node) => node.id));
  const matches = new Set(graph.nodes.filter((node) => graphNodeMatches(node, query)).map((node) => node.id));
  const retained = new Set(matches);
  for (const edge of graph.edges) {
    if (!existing.has(edge.from) || !existing.has(edge.to)) continue;
    if (matches.has(edge.from) || matches.has(edge.to)) { retained.add(edge.from); retained.add(edge.to); }
  }
  const nodes = graph.nodes.filter((node) => retained.has(node.id));
  const edges = graph.edges.filter((edge) => retained.has(edge.from) && retained.has(edge.to));
  const clusters = graph.clusters.map((cluster) => ({ ...cluster, nodes: cluster.nodes.filter((id) => retained.has(id)) }))
    .filter((cluster) => cluster.nodes.length > 0);
  return { ...graph, nodes, edges, clusters, stats: { nodes: nodes.length, edges: edges.length, clusters: clusters.length,
    connections: edges.reduce((sum, edge) => sum + edge.connections, 0) } };
}

/** Nodes with no links and no cluster: laid out in a tidy grid instead of one long rank. */
export function isolated(g: Graph): Set<string> {
  const linked = new Set(g.edges.flatMap((e) => [e.from, e.to]));
  return new Set(g.nodes.filter((n) => !n.cluster && !linked.has(n.id)).map((n) => n.id));
}

/** Layered left→right layout inside each observed component. Disconnected services and
 *  clusters are packed together instead of adding long empty ranks to the whole graph. */
export function layered(g: Graph): Positions {
  return packComponents(g, "layered");
}

const COMPONENT_GAP = 40;
const compareId = (a: { id: string }, b: { id: string }) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0;

/** Existing edges and existing cluster membership are the only associations used here.
 *  In particular, a nearby card or a matching label never creates a relationship. */
function connectedComponents(g: Graph): Graph[] {
  const parent = new Map(g.nodes.map((n) => [n.id, n.id]));
  const find = (id: string): string => {
    let root = id;
    while (parent.get(root) !== root) root = parent.get(root)!;
    while (id !== root) { const next = parent.get(id)!; parent.set(id, root); id = next; }
    return root;
  };
  const join = (a: string, b: string) => {
    if (!parent.has(a) || !parent.has(b)) return;
    const ar = find(a), br = find(b);
    if (ar !== br) parent.set(br, ar);
  };
  for (const edge of g.edges) join(edge.from, edge.to);
  const firstByCluster = new Map<string, string>();
  for (const node of g.nodes) {
    if (!node.cluster) continue;
    const first = firstByCluster.get(node.cluster);
    if (first) join(first, node.id);
    else firstByCluster.set(node.cluster, node.id);
  }
  for (const cluster of g.clusters) {
    const members = cluster.nodes.filter((id) => parent.has(id));
    for (const id of members.slice(1)) join(members[0], id);
  }
  const groups = new Map<string, Graph>();
  for (const node of g.nodes) {
    const root = find(node.id);
    if (!groups.has(root)) groups.set(root, { ...g, nodes: [], edges: [], clusters: [] });
    groups.get(root)!.nodes.push(node);
  }
  for (const edge of g.edges) {
    if (parent.has(edge.from) && parent.has(edge.to)) groups.get(find(edge.from))!.edges.push(edge);
  }
  for (const cluster of g.clusters) {
    const first = cluster.nodes.find((id) => parent.has(id));
    if (first) groups.get(find(first))!.clusters.push(cluster);
  }
  for (const group of groups.values()) {
    group.nodes.sort(compareId);
    group.edges.sort(compareId);
    group.clusters.sort(compareId);
  }
  return [...groups.values()];
}

/** Include the actual cluster hull, including its title, when packing a component. */
function componentBounds(g: Graph, positions: Positions) {
  const points = [...positions.values()];
  let x = Math.min(...points.map((p) => p.x)) - NODE_W / 2;
  let y = Math.min(...points.map((p) => p.y)) - NODE_H / 2;
  let right = Math.max(...points.map((p) => p.x)) + NODE_W / 2;
  let bottom = Math.max(...points.map((p) => p.y)) + NODE_H / 2;
  for (const cluster of g.clusters) {
    const members = cluster.nodes.map((id) => positions.get(id)).filter((p): p is Point => !!p);
    if (!members.length) continue;
    x = Math.min(x, Math.min(...members.map((p) => p.x)) - NODE_W / 2 - PAD);
    y = Math.min(y, Math.min(...members.map((p) => p.y)) - NODE_H / 2 - PAD - HEADER);
    right = Math.max(right, Math.max(...members.map((p) => p.x)) + NODE_W / 2 + PAD);
    bottom = Math.max(bottom, Math.max(...members.map((p) => p.y)) + NODE_H / 2 + PAD);
  }
  return { x, y, width: right - x, height: bottom - y };
}

function packComponents(g: Graph, mode: LayoutMode, iterations = 260): Positions {
  const blocks = connectedComponents(g).map((component) => {
    // Avoid a graph engine/simulation per isolated service, including large all-services scans.
    const positions = component.nodes.length === 1
      ? new Map([[component.nodes[0].id, { x: NODE_W / 2, y: NODE_H / 2 }]])
      : mode === "force" ? forceConnected(component, iterations) : layeredConnected(component);
    return { component, positions, bounds: componentBounds(component, positions) };
  });
  if (!blocks.length) return new Map();
  // Tall/wide blocks first makes rows compact; identity breaks ties independently of scan order.
  blocks.sort((a, b) => b.bounds.height - a.bounds.height || b.bounds.width - a.bounds.width || compareId(a.component.nodes[0], b.component.nodes[0]));
  const area = blocks.reduce((sum, b) => sum + (b.bounds.width + COMPONENT_GAP) * (b.bounds.height + COMPONENT_GAP), 0);
  const columns = Math.max(1, Math.round((Math.sqrt(area * 1.6) + COMPONENT_GAP) / (NODE_W + COMPONENT_GAP)));
  const rowWidth = Math.max(Math.max(...blocks.map((b) => b.bounds.width)), columns * (NODE_W + COMPONENT_GAP) - COMPONENT_GAP);
  const rows: { x: number; y: number; height: number }[] = [];
  const out: Positions = new Map();
  for (const block of blocks) {
    let row = rows.find((r) => r.height >= block.bounds.height && r.x + block.bounds.width <= rowWidth);
    if (!row) {
      const previous = rows.at(-1);
      row = { x: 0, y: previous ? previous.y + previous.height + COMPONENT_GAP : 0, height: block.bounds.height };
      rows.push(row);
    }
    for (const [id, point] of block.positions) {
      out.set(id, { x: row.x + point.x - block.bounds.x, y: row.y + point.y - block.bounds.y });
    }
    row.x += block.bounds.width + COMPONENT_GAP;
  }
  return out;
}

function layeredConnected(g: Graph): Positions {
  const d = new dagre.graphlib.Graph({ compound: true, multigraph: false });
  d.setGraph({ rankdir: "LR", nodesep: 34, ranksep: 110, marginx: 24, marginy: 24 });
  d.setDefaultEdgeLabel(() => ({}));
  const present = new Set(g.nodes.map((n) => n.id));
  for (const c of g.clusters) if (c.nodes.some((id) => present.has(id))) d.setNode(groupId(c.id), {});
  for (const n of g.nodes) {
    d.setNode(n.id, { width: NODE_W, height: NODE_H + (n.cluster ? 8 : 0) });
    if (n.cluster && d.hasNode(groupId(n.cluster))) d.setParent(n.id, groupId(n.cluster));
  }
  for (const e of g.edges) if (present.has(e.from) && present.has(e.to) && e.from !== e.to) d.setEdge(e.from, e.to);
  dagre.layout(d);
  const out: Positions = new Map();
  for (const n of g.nodes) {
    const p = d.node(n.id) as { x: number; y: number } | undefined;
    out.set(n.id, { x: p?.x ?? 0, y: p?.y ?? 0 });
  }
  return out;
}

/** Deterministic force-directed layout seeded from the layered one: springs along edges,
 *  repulsion between nodes, a pull towards each cluster's centroid so groups stay tight, and
 *  gravity so unconnected nodes stay on screen. */
export function force(g: Graph, iterations = 260): Positions {
  if (g.nodes.length > FORCE_NODE_LIMIT) return layered(g);
  return packComponents(g, "force", iterations);
}

function forceConnected(g: Graph, iterations: number): Positions {
  const pos = layeredConnected(g);
  const ids = g.nodes.map((n) => n.id);
  const cluster = new Map(g.nodes.map((n) => [n.id, n.cluster]));
  const k = 280;
  let temp = 60;
  for (let it = 0; it < iterations; it++) {
    const disp = new Map(ids.map((id) => [id, { x: 0, y: 0 }]));
    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        const a = pos.get(ids[i])!, b = pos.get(ids[j])!;
        let dx = a.x - b.x, dy = a.y - b.y;
        let dist = Math.hypot(dx, dy);
        if (dist < 1) { dx = 1; dy = 0.5; dist = 1.1; }
        const same = cluster.get(ids[i]) && cluster.get(ids[i]) === cluster.get(ids[j]);
        const f = ((same ? 0.7 : 1.4) * k * k) / dist;
        const da = disp.get(ids[i])!, db = disp.get(ids[j])!;
        da.x += (dx / dist) * f; da.y += (dy / dist) * f;
        db.x -= (dx / dist) * f; db.y -= (dy / dist) * f;
      }
    }
    for (const e of g.edges) {
      const a = pos.get(e.from), b = pos.get(e.to);
      if (!a || !b || e.from === e.to) continue;
      const dx = a.x - b.x, dy = a.y - b.y;
      const dist = Math.max(1, Math.hypot(dx, dy));
      const f = (dist * dist) / k;
      const da = disp.get(e.from)!, db = disp.get(e.to)!;
      da.x -= (dx / dist) * f; da.y -= (dy / dist) * f;
      db.x += (dx / dist) * f; db.y += (dy / dist) * f;
    }
    for (const c of g.clusters) {
      const members = c.nodes.filter((id) => pos.has(id));
      if (members.length < 2) continue;
      const cx = members.reduce((s, id) => s + pos.get(id)!.x, 0) / members.length;
      const cy = members.reduce((s, id) => s + pos.get(id)!.y, 0) / members.length;
      for (const id of members) {
        const p = pos.get(id)!, d = disp.get(id)!;
        d.x += (cx - p.x) * 0.8; d.y += (cy - p.y) * 0.8;
      }
    }
    // Gentle gravity beyond a radius: without it, nodes with no edges (and separate
    // components) are pushed thousands of px apart and the fitted graph looks empty.
    const gx = ids.reduce((s, id) => s + pos.get(id)!.x, 0) / ids.length;
    const gy = ids.reduce((s, id) => s + pos.get(id)!.y, 0) / ids.length;
    const radius = k * Math.sqrt(ids.length);
    for (const id of ids) {
      const p = pos.get(id)!, d = disp.get(id)!;
      const r = Math.hypot(gx - p.x, gy - p.y);
      if (r > radius) { d.x += ((gx - p.x) / r) * (r - radius) * 2; d.y += ((gy - p.y) / r) * (r - radius) * 2; }
    }
    for (const id of ids) {
      const p = pos.get(id)!, d = disp.get(id)!;
      const len = Math.max(1, Math.hypot(d.x, d.y));
      p.x += (d.x / len) * Math.min(len, temp);
      p.y += (d.y / len) * Math.min(len, temp);
    }
    temp = Math.max(2, temp * 0.97);
  }
  separate(pos, 60);
  return pos;
}

/** Push overlapping node boxes apart (a few relaxation passes). */
export function separate(pos: Positions, passes = 40, gap = 24) {
  const ids = [...pos.keys()];
  for (let p = 0; p < passes; p++) {
    let moved = false;
    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        const a = pos.get(ids[i])!, b = pos.get(ids[j])!;
        const ox = NODE_W + gap - Math.abs(a.x - b.x);
        const oy = NODE_H + gap - Math.abs(a.y - b.y);
        if (ox > 0 && oy > 0) {
          moved = true;
          if (ox < oy) { const s = (a.x < b.x ? -ox : ox) / 2; a.x += s; b.x -= s; }
          else { const s = (a.y < b.y ? -oy : oy) / 2; a.y += s; b.y -= s; }
        }
      }
    }
    if (!moved) break;
  }
}

export function layout(g: Graph, mode: LayoutMode): Positions {
  return mode === "force" ? force(g) : layered(g);
}

/** Every node reachable downstream (what `id` depends on) and upstream (what depends on it),
 *  plus the edges on those paths — used for hover highlighting. */
export function related(g: Graph, id: string): { nodes: Set<string>; edges: Set<string> } {
  const nodes = new Set([id]);
  const edges = new Set<string>();
  const down = new Map<string, GraphEdge[]>(), up = new Map<string, GraphEdge[]>();
  for (const edge of g.edges) {
    const outgoing = down.get(edge.from) ?? [];
    outgoing.push(edge);
    down.set(edge.from, outgoing);
    const incoming = up.get(edge.to) ?? [];
    incoming.push(edge);
    up.set(edge.to, incoming);
  }
  const walk = (start: string, adjacency: Map<string, GraphEdge[]>, direction: "from" | "to") => {
    const stack = [start];
    const seen = new Set([start]);
    while (stack.length) {
      const cur = stack.pop()!;
      for (const e of adjacency.get(cur) ?? []) {
        const to = e[direction];
        edges.add(e.id);
        nodes.add(to);
        if (!seen.has(to)) { seen.add(to); stack.push(to); }
      }
    }
  };
  walk(id, down, "to");
  walk(id, up, "from");
  return { nodes, edges };
}

/** Count distinct local peers in one pass, rather than scanning every edge per node. */
export function localLinkCounts(g: Graph | null): Map<string, number> {
  const peers = new Map<string, Set<string>>();
  for (const edge of g?.edges ?? []) {
    if (edge.kind !== "local") continue;
    for (const [from, to] of [[edge.from, edge.to], [edge.to, edge.from]]) {
      const adjacent = peers.get(from) ?? new Set<string>();
      adjacent.add(to);
      peers.set(from, adjacent);
    }
  }
  const counts = new Map<string, number>();
  for (const node of g?.nodes ?? []) for (const port of node.ports) counts.set(port.entry_id, peers.get(node.id)?.size ?? 0);
  return counts;
}

/** Caption under the name: framework / image / role, never repeating the title. */
export function nodeSubtitle(n: GraphNode): string {
  if (n.kind === "external") return "Internet & remote hosts";
  const parts = [n.framework?.name, n.container?.image ?? null, n.kind === "client" ? "client" : null]
    .filter((s): s is string => !!s && s !== n.label);
  if (parts.length) return parts.join(" · ");
  return n.subtitle && n.subtitle !== n.label ? n.subtitle : "";
}

export const dependents = (g: Graph, id: string): GraphNode[] =>
  uniq(g.edges.filter((e) => e.to === id && e.kind === "local").map((e) => e.from)).map((x) => g.nodes.find((n) => n.id === x)!).filter(Boolean);
export const dependencies = (g: Graph, id: string): GraphNode[] =>
  uniq(g.edges.filter((e) => e.from === id).map((e) => e.to)).map((x) => g.nodes.find((n) => n.id === x)!).filter(Boolean);
const uniq = <T,>(xs: T[]) => [...new Set(xs)];

export interface FlowNodeData extends Record<string, unknown> {
  node?: GraphNode;
  cluster?: Cluster;
  count?: number;
  dim: boolean;
  hl: boolean;
  selected: boolean;
}

export interface FlowNode {
  id: string;
  type: "service" | "cluster";
  position: Point;
  data: FlowNodeData;
  parentId?: string;
  width?: number;
  height?: number;
  zIndex?: number;
  selectable?: boolean;
  draggable?: boolean;
  class?: string;
}

export interface FlowEdgeData extends Record<string, unknown> {
  edge: GraphEdge;
  animate: boolean;
  dim: boolean;
  hl: boolean;
}

export interface FlowEdge {
  id: string;
  source: string;
  target: string;
  type: "traffic";
  data: FlowEdgeData;
  markerEnd: { type: "arrowclosed"; width: number; height: number };
  zIndex?: number;
}

export interface FlowOptions {
  focus?: string | null;
  selectedId?: string | null;
  animate?: boolean;
}

/** Convert a laid-out graph to Svelte Flow nodes (cluster groups first, children relative to
 *  their group) and edges. */
export function toFlow(g: Graph, pos: Positions, opts: FlowOptions = {}): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const rel = opts.focus ? related(g, opts.focus) : null;
  const nodes: FlowNode[] = [];
  const origin = new Map<string, Point>();
  for (const c of g.clusters) {
    const members = c.nodes.map((id) => pos.get(id)).filter((p): p is Point => !!p);
    if (!members.length) continue;
    const minX = Math.min(...members.map((p) => p.x)) - NODE_W / 2 - PAD;
    const minY = Math.min(...members.map((p) => p.y)) - NODE_H / 2 - PAD - HEADER;
    const maxX = Math.max(...members.map((p) => p.x)) + NODE_W / 2 + PAD;
    const maxY = Math.max(...members.map((p) => p.y)) + NODE_H / 2 + PAD;
    origin.set(c.id, { x: minX, y: minY });
    const dim = !!rel && !c.nodes.some((id) => rel.nodes.has(id));
    nodes.push({
      id: groupId(c.id),
      type: "cluster",
      position: { x: minX, y: minY },
      width: maxX - minX,
      height: maxY - minY,
      data: { cluster: c, count: members.length, dim, hl: false, selected: false },
      selectable: false,
      draggable: false,
      zIndex: -1,
    });
  }
  for (const n of g.nodes) {
    const p = pos.get(n.id) ?? { x: 0, y: 0 };
    const o = n.cluster ? origin.get(n.cluster) : undefined;
    nodes.push({
      id: n.id,
      type: "service",
      position: o ? { x: p.x - NODE_W / 2 - o.x, y: p.y - NODE_H / 2 - o.y } : { x: p.x - NODE_W / 2, y: p.y - NODE_H / 2 },
      ...(o ? { parentId: groupId(n.cluster!) } : {}),
      width: NODE_W,
      height: NODE_H,
      data: { node: n, dim: !!rel && !rel.nodes.has(n.id), hl: !!rel && rel.nodes.has(n.id), selected: opts.selectedId === n.id },
    });
  }
  const edges: FlowEdge[] = g.edges
    .filter((e) => pos.has(e.from) && pos.has(e.to))
    .map((e) => ({
      id: e.id,
      source: e.from,
      target: e.to,
      type: "traffic" as const,
      data: { edge: e, animate: opts.animate ?? true, dim: !!rel && !rel.edges.has(e.id), hl: !!rel && rel.edges.has(e.id) },
      markerEnd: { type: "arrowclosed" as const, width: 16, height: 16 },
      zIndex: 1,
    }));
  return { nodes, edges };
}

/** The service order from a cluster plan summary ("… in dependency order: web → api → db."). */
export function orderFromSummary(summary: string): string[] {
  const m = summary.match(/dependency order: (.*?)\.?$/);
  return m ? m[1].split(" → ").map((s) => s.trim()).filter(Boolean) : [];
}

/** The graph node that owns a port entry. */
export function nodeForEntry(g: Graph | null, entryId: string): GraphNode | null {
  return g?.nodes.find((n) => n.ports.some((p) => p.entry_id === entryId)) ?? null;
}

export interface ClusterSection {
  id: string;
  title: string;
  hint: string;
  items: PortEntry[];
}

/** Group list entries by topology cluster (ungrouped last), for the list's "Cluster" mode. */
export function sectionsByCluster(entries: PortEntry[], g: Graph | null): ClusterSection[] {
  const byEntry = new Map<string, Cluster>();
  for (const c of g?.clusters ?? []) {
    for (const id of c.nodes) {
      const n = g!.nodes.find((x) => x.id === id);
      for (const p of n?.ports ?? []) byEntry.set(p.entry_id, c);
    }
  }
  const sections = new Map<string, ClusterSection>();
  const rest: PortEntry[] = [];
  for (const e of entries) {
    const c = byEntry.get(e.id);
    if (!c) { rest.push(e); continue; }
    const s = sections.get(c.id) ?? { id: c.id, title: c.name, hint: `${CLUSTER_LABEL[c.kind]}${c.detail ? ` · ${c.detail}` : ""}`, items: [] };
    s.items.push(e);
    sections.set(c.id, s);
  }
  const out = [...sections.values()];
  if (rest.length) out.push({ id: "__ungrouped", title: out.length ? "Not in a cluster" : "", hint: "", items: rest });
  return out;
}

/** "1 link" / "6 links": what the Graph view's count badge means. */
export const linksLabel = (n: number): string => `${n} link${n === 1 ? "" : "s"}`;

/** Edge label: ":5432 ×3" or the remote host for outbound edges. */
export function edgeLabel(e: GraphEdge): string {
  if (e.kind === "outbound") {
    const hosts = e.remotes.slice(0, 2).join(", ");
    return `${hosts}${e.remotes.length > 2 ? ` +${e.remotes.length - 2}` : ""}`;
  }
  return `:${e.port}${e.connections > 1 ? ` ×${e.connections}` : ""}`;
}

/** Approximate advance of one mono caption glyph (11 px) and the label's horizontal chrome. */
const LABEL_CH = 6.7;
const LABEL_CHROME = 14;
/** Room kept clear at each end of an edge for the arrowhead and the node border. */
const LABEL_CLEAR = 14;

/**
 * Fits an edge label between the two handles so it never covers a node: on a mostly horizontal
 * edge the label (centred on the midpoint) gets the gap minus the arrow clearance; vertical edges
 * pass beside the nodes, so only the 180 px cap applies. Clipped labels end in "…".
 */
export function fitEdgeLabel(text: string, sx: number, sy: number, tx: number, ty: number): { text: string; clipped: boolean } {
  const dx = Math.abs(tx - sx), dy = Math.abs(ty - sy);
  const room = Math.min(180, dx >= dy ? dx - 2 * LABEL_CLEAR : 180);
  const chars = Math.max(3, Math.floor((room - LABEL_CHROME) / LABEL_CH));
  if (text.length <= chars) return { text, clipped: false };
  return { text: `${text.slice(0, chars - 1).trimEnd()}…`, clipped: true };
}

/** Dots travelling along an edge: more connections → more dots (1–4), faster when busy. */
export function trafficDots(connections: number): { count: number; duration: number } {
  const count = Math.max(1, Math.min(4, Math.ceil(Math.log2(connections + 1))));
  return { count, duration: Math.max(1.4, 3.2 - connections * 0.2) };
}

/** A service card's accessible name: the words on the card in reading order (name, up to three
 *  ports, overflow, subtitle), then facts shown only as icons or figures. */
export function serviceLabel(n: GraphNode, usage: { cpu: string; memory: string } | null): string {
  const ports = n.ports.slice(0, 3).map((p) => `:${p.port}`);
  if (n.ports.length > 3) ports.push(`+${n.ports.length - 3}`);
  const sub = nodeSubtitle(n);
  const shown = [n.label, ...ports, ...(sub ? [sub] : [])].join(" ");
  const facts = [
    n.protected ? "protected" : null,
    n.ports.some((p) => p.exposure === "all_interfaces") ? "reachable from the network" : null,
    usage ? `CPU ${usage.cpu}, memory ${usage.memory}` : null,
  ].filter(Boolean);
  return [shown, ...facts].join(", ");
}
