import { activityMatches, callGraph, isIncomplete, spanNodeId } from "./build";
import type { ActivityRun } from "./build";
import type { ActivitySpan, VisualGraph } from "./model";

export type ActivityFilter = "all" | "failed" | "incomplete";
export type ActivityOrder = "time" | "duration";

const chronological = (a: ActivitySpan, b: ActivitySpan) =>
  (a.start_at_ms ?? a.end_at_ms ?? Number.MAX_SAFE_INTEGER) - (b.start_at_ms ?? b.end_at_ms ?? Number.MAX_SAFE_INTEGER) || a.id.localeCompare(b.id);

/** Filter recorded metadata without changing source outcomes or estimating missing durations. */
export function activityRows(run: ActivityRun | null, query = "", filter: ActivityFilter = "all", order: ActivityOrder = "time"): ActivitySpan[] {
  return (run?.spans ?? []).filter((span) => activityMatches(span, query)
    && (filter === "all" || (filter === "failed" ? span.status === "error" : isIncomplete(span))))
    .sort((a, b) => order === "duration"
      ? (b.duration_ms ?? -1) - (a.duration_ms ?? -1) || chronological(a, b)
      : chronological(a, b));
}

/** Keep matching spans, inspected records and their explicitly reported parent context. */
export function activityContext(run: ActivityRun | null, rows: ActivitySpan[], chosen: string | null): VisualGraph {
  const graph = callGraph(run);
  const present = new Set(graph.nodes.map((node) => node.id));
  const parents = new Map(graph.edges.map((edge) => [edge.to, edge.from]));
  const included = new Set<string>();
  const include = (start: string) => {
    let current: string | undefined = start;
    while (current && present.has(current) && !included.has(current)) {
      included.add(current);
      current = parents.get(current);
    }
  };
  for (const row of rows) include(spanNodeId(row));
  if (chosen) {
    include(chosen);
    const edge = graph.edges.find((edge) => edge.id === chosen);
    if (edge) { include(edge.from); include(edge.to); }
  }
  return {
    nodes: graph.nodes.filter((node) => included.has(node.id)),
    edges: graph.edges.filter((edge) => included.has(edge.from) && included.has(edge.to)),
  };
}
