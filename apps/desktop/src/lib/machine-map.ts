// Machine map view model: each agent's footprint (observed project folders, listening ports,
// configured MCP servers, remote hosts) and a radial layout that fans it out in the agent's sector.
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

/** Elliptical stretch so the map fills a wide stage. */
export const MAP_STRETCH = { x: 1.45, y: 0.75 };

/** Radial layout: agents on an inner ring, each agent's footprint fanned out in its own sector. */
export function radialLayout(agents: Agent[], inner = 150, outer = 300, max = 14, stretch = MAP_STRETCH) {
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
      return { item, angle: a, x: Math.cos(a) * r * stretch.x, y: Math.sin(a) * r * stretch.y };
    });
    return { agent, angle, x: Math.cos(angle) * inner * stretch.x, y: Math.sin(angle) * inner * stretch.y, nodes: placed };
  });
}
