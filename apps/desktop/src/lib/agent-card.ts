// Agent card view model for Home: fixed counters (projects, ports, MCP, skills) and chips, all from
// the scan and the agent's own configuration. Unknown stays unknown (null), never zero. MCP chips
// carry env/header names only, never values.
import { configStatus, isUnpinned } from "./agent-config";
import { stoppableEntries } from "./agents";
import type { FootprintNode } from "./machine-map";
import type { Agent, PortEntry } from "./types";

export interface CardPort { entryId: string; port: number; label: string; warn: boolean }
export interface CardMcp { id: string; name: string; running: boolean; warn: boolean; names: string[] }
export interface CardModel {
  projects: string[];
  ports: CardPort[];
  /** null: configuration not read (off) or not readable for this agent. */
  mcp: { total: number; running: number; servers: CardMcp[] } | null;
  skills: number | null;
  /** Why MCP/skills are unknown, when they are. */
  unknownReason: string | null;
  configOff: boolean;
  stoppable: number;
}

export function cardModel(agent: Agent, entries: PortEntry[] | null): CardModel {
  const st = configStatus(agent);
  const caps = agent.capabilities;
  const read = st.kind === "read" && caps;
  return {
    projects: [...new Set(agent.folders.filter((f) => f.source !== "recent").map((f) => f.project?.name ?? f.label))],
    ports: agent.ports.map((p) => ({ entryId: p.entry_id, port: p.port, label: p.framework ?? p.project ?? p.label, warn: p.exposure === "all_interfaces" })),
    mcp: read ? {
      total: st.summary.mcp,
      running: st.summary.running,
      servers: caps.mcp_servers.filter((s) => s.enabled !== false).map((s) => ({
        id: s.id, name: s.name, running: s.runtime?.state === "running", warn: isUnpinned(s),
        names: [...(s.env_names ?? []), ...(s.header_names ?? [])],
      })),
    } : null,
    skills: read ? st.summary.skills : null,
    unknownReason: st.kind === "unsupported" ? st.reason : null,
    configOff: st.kind === "off",
    stoppable: entries ? stoppableEntries(agent, entries).length : 0,
  };
}

export interface MiniNode { node: FootprintNode; x: number; y: number }
const ARC: Record<FootprintNode["kind"], number> = { project: -Math.PI / 2, port: 0, mcp: Math.PI / 2, remote: Math.PI };

/** Small constellation: each kind gets its own quadrant (projects top, ports right, MCP bottom,
 *  remote left) so the shape of a footprint reads at a glance and kinds never interleave. */
export function miniLayout(nodes: FootprintNode[], r = 40): MiniNode[] {
  const out: MiniNode[] = [];
  for (const kind of Object.keys(ARC) as FootprintNode["kind"][]) {
    const group = nodes.filter((n) => n.kind === kind);
    const spread = Math.min(Math.PI / 2 * 0.8, (group.length - 1) * 0.36);
    group.forEach((node, i) => {
      const a = ARC[kind] - spread / 2 + (group.length > 1 ? (spread * i) / (group.length - 1) : 0);
      const rr = r + (group.length > 3 && i % 2 ? 10 : 0);
      out.push({ node, x: Math.cos(a) * rr, y: Math.sin(a) * rr });
    });
  }
  return out;
}
