// Scope observed flow, reported activity and graph peers to one agent. Ownership comes from the
// agents report (process IDs) or a span's reported agent / provider; nothing here is inferred
// from timing or names alone.
import type { ActivitySpan, AgentFlowEdge, AgentFlowNode, AgentFlowReport } from "./agent-flow/model";
import type { ActivityRun } from "./agent-flow/build";
import type { Agent, PortEntry } from "./types";

/** Every process ID the report attributes to the agent, including omitted processes and tools. */
export function agentPids(agent: Agent): Set<number> {
  return new Set([agent.pid, ...(agent.process_ids ?? []), ...agent.processes.map((p) => p.pid), ...(agent.tools ?? []).map((t) => t.pid)]);
}

/** A reporting provider and the agent product it belongs to (reported metadata, not observation). */
const PROVIDER_PRODUCT: Partial<Record<ActivitySpan["provider"], string[]>> = {
  claude: ["claude-code", "claude"],
  codex: ["codex", "codex-cli"],
};

/** Whether a recorded span was reported for this agent (by id, by an owned PID or by its provider). */
export function spanBelongsTo(span: ActivitySpan, agent: Agent, pids = agentPids(agent)): boolean {
  if (span.agent_id && (span.agent_id === agent.id || span.agent_id === agent.product)) return true;
  if (span.pid != null && pids.has(span.pid)) return true;
  return !!PROVIDER_PRODUCT[span.provider]?.includes(agent.product);
}

/** Recorded runs that contain at least one span reported for the agent. */
export function agentRuns(runs: ActivityRun[], agent: Agent): ActivityRun[] {
  const pids = agentPids(agent);
  return runs.filter((run) => run.spans.some((s) => spanBelongsTo(s, agent, pids)));
}

export function flowNodeBelongsTo(node: AgentFlowNode, agent: Agent, pids = agentPids(agent)): boolean {
  return node.agent_id === agent.id || (node.pid != null && pids.has(node.pid));
}

/** The agent's own flow nodes plus their direct neighbours; edges only between kept nodes. */
export function scopeFlowReport(report: AgentFlowReport | null, agent: Agent): AgentFlowReport | null {
  if (!report) return null;
  const pids = agentPids(agent);
  const own = new Set(report.nodes.filter((n) => flowNodeBelongsTo(n, agent, pids)).map((n) => n.id));
  const keep = new Set(own);
  for (const e of report.edges) {
    if (own.has(e.from)) keep.add(e.to);
    if (own.has(e.to)) keep.add(e.from);
  }
  return { ...report, nodes: report.nodes.filter((n) => keep.has(n.id)), edges: report.edges.filter((e) => keep.has(e.from) && keep.has(e.to)) };
}

export interface AgentSocketLink { edge: AgentFlowEdge; from: AgentFlowNode; to: AgentFlowNode }

/** Observed live TCP relationships that start or end at one of the agent's processes. */
export function agentSocketLinks(report: AgentFlowReport | null, agent: Agent): AgentSocketLink[] {
  if (!report) return [];
  const pids = agentPids(agent);
  const byId = new Map(report.nodes.map((n) => [n.id, n]));
  return report.edges.flatMap((edge) => {
    const from = byId.get(edge.from), to = byId.get(edge.to);
    if (edge.kind !== "tcp" || !from || !to) return [];
    return flowNodeBelongsTo(from, agent, pids) || flowNodeBelongsTo(to, agent, pids) || edge.source_pids.some((p) => pids.has(p)) ? [{ edge, from, to }] : [];
  });
}

/** The running agent that owns one of these process IDs, if any. */
export function agentForPids(agents: Agent[] | null | undefined, pids: (number | null | undefined)[]): Agent | null {
  const wanted = pids.filter((p): p is number => p != null);
  if (!agents?.length || !wanted.length) return null;
  return agents.find((a) => { const own = agentPids(a); return wanted.some((p) => own.has(p)); }) ?? null;
}

/** The running agent that started or serves this port entry, if any. */
export function agentForEntry(agents: Agent[] | null | undefined, entry: PortEntry | null): Agent | null {
  if (!entry || !agents?.length) return null;
  return agents.find((a) => a.ports.some((p) => p.entry_id === entry.id)) ?? agentForPids(agents, [entry.pid, ...(entry.pids ?? [])]);
}
