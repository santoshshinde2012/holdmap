import type { ActivitySpan, AgentFlowReport, VisualGraph, VisualNode } from "./model";
import { PROVIDER_LABEL, STATUS_LABEL } from "./model";

export interface ActivityRun {
  id: string;
  kind: "trace" | "session" | "event";
  label: string;
  spans: ActivitySpan[];
  calls: number;
  errors: number;
  first_at_ms: number | null;
}

function metadataMatches(query: string, text: string, pid: number | null, port: number | null = null): boolean {
  return query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean).every((term) => {
    const exactPid = /^pid:(\d+)$/.exec(term);
    if (exactPid) return pid === Number(exactPid[1]);
    const exactPort = /^(?:port:|:)(\d+)$/.exec(term);
    if (exactPort) return port === Number(exactPort[1]);
    if (/^\d+$/.test(term)) return pid === Number(term) || port === Number(term);
    return text.toLocaleLowerCase().includes(term);
  });
}

export function observedGraph(report: AgentFlowReport | null, query = ""): VisualGraph {
  if (!report) return { nodes: [], edges: [] };
  const all = new Map(report.nodes.map((n) => [n.id, n]));
  const matching = new Set(report.nodes.filter((n) => metadataMatches(query, [n.label, n.kind, n.tool_kind?.replaceAll("_", " "), n.address, n.protocol].filter(Boolean).join(" "), n.pid, n.port)).map((n) => n.id));
  const owners = new Set(report.nodes.filter((n) => n.kind === "agent" && matching.has(n.id)).map((n) => n.agent_id).filter(Boolean));
  for (const n of report.nodes) if (n.agent_id && owners.has(n.agent_id)) matching.add(n.id);
  // Keep adjacent context, never invent an edge to a filtered or absent process.
  const visible = new Set(matching);
  for (const e of report.edges) {
    if (matching.has(e.from)) visible.add(e.to);
    if (matching.has(e.to)) visible.add(e.from);
  }
  return {
    nodes: [...all.values()].filter((n) => visible.has(n.id)).map((n) => ({
      id: n.id, label: n.label, kind: n.kind, evidence: n.evidence, observed: n,
      sub: n.kind === "listener" ? `${n.protocol?.toUpperCase() ?? "Protocol unknown"} · ${n.address ?? "Address unknown"}:${n.port ?? "?"}`
        : n.kind === "remote" ? `${n.address ?? "Endpoint unknown"}:${n.port ?? "?"}`
          : n.pid != null ? `PID ${n.pid}${n.tool_kind ? ` · ${n.tool_kind.replaceAll("_", " ")}` : ""}` : "Identity unavailable",
    })),
    edges: report.edges.filter((e) => visible.has(e.from) && visible.has(e.to) && all.has(e.from) && all.has(e.to)).map((e) => ({
      id: e.id, from: e.from, to: e.to, kind: e.kind, evidence: e.evidence, observed: e,
      label: e.kind === "parent" ? "Parent process" : e.kind === "owns_listener" ? "Holds listener"
        : `${e.connections} live ${e.connections === 1 ? "socket" : "sockets"}`,
    })),
  };
}

export function activityRuns(spans: ActivitySpan[], query = ""): ActivityRun[] {
  const groups = new Map<string, ActivityRun>();
  for (const span of spans) {
    const kind = span.trace_id ? "trace" : span.session_id ? "session" : "event";
    const id = `${kind}:${span.trace_id || span.session_id || span.id}`;
    let run = groups.get(id);
    if (!run) {
      run = { id, kind, label: kind === "trace" ? "Recorded trace" : kind === "session" ? "Recorded session" : "Uncorrelated event", spans: [], calls: 0, errors: 0, first_at_ms: null };
      groups.set(id, run);
    }
    run.spans.push(span);
    if (span.kind === "tool") run.calls++;
    if (span.status === "error") run.errors++;
    const time = span.start_at_ms ?? span.end_at_ms;
    if (time != null && (run.first_at_ms == null || time < run.first_at_ms)) run.first_at_ms = time;
  }
  return [...groups.values()].filter((run) => run.spans.some((s) => metadataMatches(query, [s.name, s.kind, PROVIDER_LABEL[s.provider], STATUS_LABEL[s.status]].join(" "), s.pid)))
    .sort((a, b) => (b.first_at_ms ?? -1) - (a.first_at_ms ?? -1) || a.id.localeCompare(b.id));
}

export const spanNodeId = (span: ActivitySpan) => `span:${span.trace_id || span.session_id || "event"}:${span.id}`;

export function callGraph(run: ActivityRun | null): VisualGraph {
  if (!run) return { nodes: [], edges: [] };
  const spans = new Map(run.spans.map((s) => [s.id, s]));
  return {
    nodes: [...spans.values()].map((s) => ({ id: spanNodeId(s), label: s.name, kind: s.kind, evidence: "unknown", status: s.status, span: s,
      sub: `${PROVIDER_LABEL[s.provider]} · ${STATUS_LABEL[s.status]}` })),
    edges: run.spans.flatMap((s) => {
      const parent = s.parent_id ? spans.get(s.parent_id) : undefined;
      // Session grouping and timestamps describe membership, not communication.
      if (!s.trace_id || !parent || parent.trace_id !== s.trace_id || parent.id === s.id) return [];
      return [{ id: `parent:${spanNodeId(s)}`, from: spanNodeId(parent), to: spanNodeId(s), kind: "span_parent" as const,
        evidence: "unknown" as const, label: "Reported parent" }];
    }),
  };
}

export const CALL_GRAPH_LIMIT = 128;

/** Keep the complete timeline; bound expensive canvas work and retain inspected context. */
export function graphWindow(graph: VisualGraph, chosen: string | null, limit = CALL_GRAPH_LIMIT): { graph: VisualGraph; omitted: number } {
  const maximum = Math.max(1, Math.floor(limit));
  if (graph.nodes.length <= maximum) return { graph, omitted: 0 };
  const present = new Set(graph.nodes.map((n) => n.id));
  const parents = new Map(graph.edges.filter((e) => e.kind === "span_parent").map((e) => [e.to, e.from]));
  const included = new Set<string>();
  let current = chosen;
  while (current && present.has(current) && !included.has(current) && included.size < maximum) {
    included.add(current); current = parents.get(current) ?? null;
  }
  for (const node of graph.nodes) {
    if (included.size === maximum) break;
    included.add(node.id);
  }
  const nodes = graph.nodes.filter((n) => included.has(n.id));
  return { graph: { nodes, edges: graph.edges.filter((e) => included.has(e.from) && included.has(e.to)) }, omitted: graph.nodes.length - nodes.length };
}

export function missingParent(span: ActivitySpan, run: ActivityRun): boolean {
  return !!span.parent_id && !run.spans.some((s) => s.id === span.parent_id && !!s.trace_id && s.trace_id === span.trace_id);
}

export const isIncomplete = (span: ActivitySpan) => span.end_at_ms == null && (span.status === "requested" || span.status === "running" || span.status === "unknown");

export function duration(span: ActivitySpan): string {
  const n = span.duration_ms;
  if (n == null) return isIncomplete(span) ? "No completion received" : "Duration not provided";
  return n < 1000 ? `${Math.round(n)} ms` : `${(n / 1000).toFixed(n < 10000 ? 2 : 1)} s`;
}

export function payloadSize(n: number | null): string {
  if (n == null) return "Not provided";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MiB`;
}

export function graphSignature(graph: VisualGraph): string {
  return JSON.stringify([graph.nodes.map((n) => [n.id, n.kind]).sort(), graph.edges.map((e) => [e.id, e.from, e.to]).sort()]);
}

export function nodeDescription(node: VisualNode): string {
  return `${node.label}, ${node.kind}, ${node.sub}${node.span ? ", reported by integration" : `, ${node.evidence}`}`;
}
