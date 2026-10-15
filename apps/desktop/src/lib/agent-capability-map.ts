import { enabledLabel, scopeLabel, type CapabilityItem, type RunningMcpItem } from "./agent-capabilities";
import type { Agent } from "./types";

export const CAPABILITY_MAP_LIMITS = { agents: 24, inventory: 64, running: 24 } as const;
export const CAPABILITY_MAP_NODE_WIDTH = 252;
export const CAPABILITY_MAP_NODE_HEIGHT = 104;
const HEADER = 72;
const PAD = 20;
const ROW_GAP = 16;
const LANE_GAP = 28;
const LANE_WIDTH = CAPABILITY_MAP_NODE_WIDTH + PAD * 2;
const COLUMN_GAP = 100;

export type CapabilityGraphKind = "agent" | "mcp" | "skill" | "running";
export type CapabilityInventoryState = "not_reported" | "not_inspected" | "inspected";
export type CapabilityGraphPoint = { x: number; y: number };
export type CapabilityGraphNode = {
  id: string;
  kind: CapabilityGraphKind;
  label: string;
  sub: string;
  position: CapabilityGraphPoint;
  width: number;
  height: number;
  /** Exact observed owners, including owners omitted by the map's display bound. */
  ownerIds: string[];
  emptyHint: string | null;
  agent?: Agent;
  item?: CapabilityItem;
  runtime?: RunningMcpItem;
  inventoryState?: CapabilityInventoryState;
};
export type CapabilityGraphEdge = {
  id: string;
  from: string;
  to: string;
  sourceHandle: "mcp" | "skill";
  kind: "definition";
  label: "Configuration observed" | "Skill file observed";
};
export type CapabilityGraphLane = {
  id: "agents" | "mcp" | "skill" | "running";
  label: string;
  sub: string;
  x: number;
  y: number;
  width: number;
  height: number;
  count: number;
  total: number;
};
export type CapabilityGraphCounts = { agents: number; inventory: number; running: number };
export type CapabilityGraph = {
  nodes: CapabilityGraphNode[];
  edges: CapabilityGraphEdge[];
  /** Full lane rectangles; cards start below the reserved 72px header. */
  lanes: CapabilityGraphLane[];
  /** Top-left card positions, matching node.position. */
  positions: Map<string, CapabilityGraphPoint>;
  /** Layout/association identity only; resource samples do not change it. */
  signature: string;
  bounds: { width: number; height: number };
  total: CapabilityGraphCounts;
  shown: CapabilityGraphCounts;
  omitted: CapabilityGraphCounts & { ownerEdges: number };
};
export type CapabilityMapInput = {
  agents: readonly Agent[];
  /** Already filtered by search, kind, scope and configured state. */
  items: readonly CapabilityItem[];
  running: readonly RunningMcpItem[];
  selectedAgent?: string | null;
  selectedId?: string | null;
  /** Hide unrelated agents, retaining explicit agent selection even with zero matches. */
  filtered?: boolean;
};

const compare = (a: string, b: string): number => a < b ? -1 : a > b ? 1 : 0;
const byName = (a: { name: string; id: string }, b: { name: string; id: string }): number =>
  compare(a.name.toLowerCase(), b.name.toLowerCase()) || compare(a.name, b.name) || compare(a.id, b.id);

/** Reuse inventory IDs; neither names nor runtime process labels establish an association. */
function sharedRows<T extends { id: string; name: string; owners: Agent[] }>(rows: readonly T[], agents: Map<string, Agent>): T[] {
  const found = new Map<string, T>();
  for (const row of rows) {
    const owners = new Map<string, Agent>();
    for (const owner of [...(found.get(row.id)?.owners ?? []), ...row.owners]) {
      const current = agents.get(owner.id);
      if (current) owners.set(current.id, current);
    }
    // Sources outside the supplied agent context never create phantom agents or orphan rows.
    if (owners.size) found.set(row.id, { ...row, owners: [...owners.values()].sort(byName) });
  }
  return [...found.values()].sort(byName);
}

function retain<T extends { id: string }>(rows: T[], maximum: number, selectedId?: string | null): T[] {
  const chosen = selectedId ? rows.find((row) => row.id === selectedId) : undefined;
  const kept = new Set(rows.slice(0, maximum).map((row) => row.id));
  if (chosen && !kept.has(chosen.id)) {
    const last = rows[Math.min(maximum, rows.length) - 1];
    if (last) kept.delete(last.id);
    kept.add(chosen.id);
  }
  return rows.filter((row) => kept.has(row.id));
}

function agentHint(agent: Agent, visibleDefinitions: number, filtered: boolean): string {
  const inventory = agent.capabilities;
  if (!inventory) return "Inventory not reported";
  if (!inventory.inspected) return "Sources not inspected";
  const omitted = inventory.more_mcp_servers + inventory.more_skills;
  if (!inventory.mcp_servers.length && !inventory.skills.length) {
    return omitted ? `${omitted} entries omitted by discovery` : "No entries reported";
  }
  if (filtered && !visibleDefinitions) return "No matching definitions";
  return `${inventory.mcp_servers.length} MCP · ${inventory.skills.length} skills${omitted ? ` · +${omitted} omitted` : ""}`;
}

/** Bounded, pure ownership map. Runtime observations deliberately have no graph edges. */
export function buildCapabilityMap(input: CapabilityMapInput): CapabilityGraph {
  const agentsById = new Map(input.agents.map((agent) => [agent.id, agent]));
  const items = sharedRows(input.items, agentsById);
  const running = sharedRows(input.running, agentsById);
  const ownerIds = new Set([...items, ...running].flatMap((row) => row.owners.map((owner) => owner.id)));
  const explicitAgents = new Set([input.selectedAgent, input.selectedId].filter((id): id is string => !!id && agentsById.has(id)));
  const eligibleAgents = [...agentsById.values()].filter((agent) => !input.filtered || ownerIds.has(agent.id) || explicitAgents.has(agent.id)).sort(byName);
  const selectedRow = [...items, ...running].find((row) => row.id === input.selectedId);
  const priority = new Set([...explicitAgents, ...(selectedRow?.owners.map((owner) => owner.id) ?? [])]);
  const prioritizedAgents = [...priority].map((id) => agentsById.get(id)).filter((agent): agent is Agent => !!agent);
  const agentIds = new Set([...prioritizedAgents, ...eligibleAgents.filter((agent) => !priority.has(agent.id))]
    .slice(0, CAPABILITY_MAP_LIMITS.agents).map((agent) => agent.id));
  const intersects = (row: CapabilityItem | RunningMcpItem) => row.owners.some((owner) => agentIds.has(owner.id));
  const shownItems = retain(items.filter(intersects), CAPABILITY_MAP_LIMITS.inventory, input.selectedId);
  const shownRunning = retain(running.filter(intersects), CAPABILITY_MAP_LIMITS.running, input.selectedId);
  const shownOwners = new Set([...shownItems, ...shownRunning].flatMap((row) => row.owners.map((owner) => owner.id)));
  const shownAgents = eligibleAgents.filter((agent) => agentIds.has(agent.id) && (!input.filtered || shownOwners.has(agent.id) || explicitAgents.has(agent.id)));
  const nodes: CapabilityGraphNode[] = [];
  const edges: CapabilityGraphEdge[] = [];
  const positions = new Map<string, CapabilityGraphPoint>();
  const lanes: CapabilityGraphLane[] = [];
  const makeNode = (node: Omit<CapabilityGraphNode, "position" | "width" | "height">): CapabilityGraphNode => ({
    ...node, position: { x: 0, y: 0 }, width: CAPABILITY_MAP_NODE_WIDTH, height: CAPABILITY_MAP_NODE_HEIGHT,
  });
  const itemNodes = shownItems.map((item) => makeNode({ id: item.id, kind: item.kind, label: item.name,
    sub: `${scopeLabel[item.scope]} · ${enabledLabel(item)}`, item, ownerIds: item.owners.map((owner) => owner.id).sort(compare), emptyHint: null }));
  const runtimeNodes = shownRunning.map((runtime) => makeNode({ id: runtime.id, kind: "running", label: runtime.name,
    sub: `PID ${runtime.tool.pid} · Runtime candidate`, runtime, ownerIds: runtime.owners.map((owner) => owner.id).sort(compare), emptyHint: null }));
  const agentNodes = shownAgents.map((agent) => makeNode({ id: agent.id, kind: "agent", label: agent.name,
    sub: `PID ${agent.pid}`, agent, ownerIds: [],
    inventoryState: !agent.capabilities ? "not_reported" : agent.capabilities.inspected ? "inspected" : "not_inspected",
    emptyHint: agentHint(agent, items.filter((item) => item.owners.some((owner) => owner.id === agent.id)).length, !!input.filtered) }));

  function lane(id: CapabilityGraphLane["id"], label: string, sub: string, members: CapabilityGraphNode[], total: number, x: number, y: number) {
    const content = Math.max(48, members.length * (CAPABILITY_MAP_NODE_HEIGHT + ROW_GAP) - ROW_GAP);
    const height = HEADER + PAD * 2 + content;
    lanes.push({ id, label, sub, x, y, width: LANE_WIDTH, height, count: members.length, total });
    members.forEach((node, index) => {
      node.position = { x: x + PAD, y: y + HEADER + PAD + index * (CAPABILITY_MAP_NODE_HEIGHT + ROW_GAP) };
      positions.set(node.id, node.position);
      nodes.push(node);
    });
    return y + height + LANE_GAP;
  }
  // Parallel categories keep typical maps readable; empty categories consume no column.
  const mcpNodes = itemNodes.filter((node) => node.kind === "mcp");
  const skillNodes = itemNodes.filter((node) => node.kind === "skill");
  let x = 0;
  if (mcpNodes.length) {
    lane("mcp", "MCP servers", "Configured definitions · use not established", mcpNodes, items.filter((item) => item.kind === "mcp").length, x, 0);
    x += LANE_WIDTH + COLUMN_GAP;
  }
  if (agentNodes.length) {
    const belowAgents = lane("agents", "Agents", "Observed processes · inventory coverage varies", agentNodes, eligibleAgents.length, x, 0);
    if (runtimeNodes.length) lane("running", "Runtime candidates", "Observed processes · separate from definitions", runtimeNodes, running.length, x, belowAgents);
    x += LANE_WIDTH + COLUMN_GAP;
  }
  if (skillNodes.length) lane("skill", "Skills", "Skill files present · activation unknown", skillNodes, items.filter((item) => item.kind === "skill").length, x, 0);

  const renderedAgents = new Set(shownAgents.map((agent) => agent.id));
  for (const item of shownItems) for (const owner of item.owners) {
    if (!renderedAgents.has(owner.id)) continue;
    edges.push({ id: `definition:${JSON.stringify([owner.id, item.id])}`, from: owner.id, to: item.id,
      sourceHandle: item.kind, kind: "definition", label: item.kind === "mcp" ? "Configuration observed" : "Skill file observed" });
  }
  edges.sort((a, b) => compare(a.id, b.id));
  const total = { agents: eligibleAgents.length, inventory: items.length, running: running.length };
  const shown = { agents: shownAgents.length, inventory: shownItems.length, running: shownRunning.length };
  return {
    nodes, edges, lanes, positions,
    signature: JSON.stringify([nodes.map((node) => [node.id, node.kind, node.position.x, node.position.y, node.ownerIds]), edges.map((edge) => edge.id)]),
    bounds: { width: Math.max(0, ...lanes.map((entry) => entry.x + entry.width)), height: Math.max(0, ...lanes.map((entry) => entry.y + entry.height)) },
    total, shown, omitted: { agents: total.agents - shown.agents, inventory: total.inventory - shown.inventory, running: total.running - shown.running,
      ownerEdges: items.reduce((sum, item) => sum + item.owners.length, 0) - edges.length },
  };
}
