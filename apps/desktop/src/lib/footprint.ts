// One agent's reach, for the Home card constellation: projects, ports, MCP servers and remote hosts.
// The Agents map draws the same facts in its own graph. This module does not lay out a second picture.
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

const LABEL_MAX = 24;

/** Shortens `text` in the middle, so both ends of a host or path stay readable. */
export function shorten(text: string, max = LABEL_MAX): string {
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

/** Everything one agent reaches, in a fixed kind order.
 *  Two or more remote hosts collapse into one count node unless `expanded` holds the group id. */
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
