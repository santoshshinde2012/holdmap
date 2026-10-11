import type { Edge, Node } from "@xyflow/svelte";
import type { VisualEdge, VisualGraph, VisualNode } from "./model";
import { nodeDescription } from "./build";

export type FlowNodeData = { node: VisualNode; chosen: boolean };
export type FlowEdgeData = { edge: VisualEdge; chosen: boolean };
export type FlowNode = Node<FlowNodeData>;
export type FlowEdge = Edge<FlowEdgeData>;

export function layoutGraph(graph: VisualGraph, calls: boolean): Map<string, { x: number; y: number }> {
  const parents = new Map(graph.edges.filter((e) => e.kind === "span_parent").map((e) => [e.to, e.from]));
  const columns: Record<string, number> = { agent: 0, tool: 1, process: 1, listener: 2, remote: 3, unknown: 3 };
  function depth(id: string): number {
    const seen = new Set([id]);
    let parent = parents.get(id), n = 0;
    while (parent && !seen.has(parent) && n < 8) { seen.add(parent); n++; parent = parents.get(parent); }
    return n;
  }
  const groups = new Map<number, VisualNode[]>();
  for (const node of graph.nodes) {
    const col = calls ? depth(node.id) : (columns[node.kind] ?? 1);
    groups.set(col, [...(groups.get(col) ?? []), node]);
  }
  const positions = new Map<string, { x: number; y: number }>();
  for (const [col, nodes] of groups) {
    // Stable IDs keep nodes in place while status and socket counts refresh.
    nodes.sort((a, b) => a.id.localeCompare(b.id)).forEach((n, row) => positions.set(n.id, { x: col * 340, y: row * 108 }));
  }
  return positions;
}

export function toFlow(graph: VisualGraph, positions: Map<string, { x: number; y: number }>, chosen: string | null): { nodes: FlowNode[]; edges: FlowEdge[] } {
  return {
    nodes: graph.nodes.map((n) => ({ id: n.id, type: "activity", position: positions.get(n.id) ?? { x: 0, y: 0 }, width: 244, height: 78,
      data: { node: n, chosen: chosen === n.id }, draggable: false, connectable: false,
      ariaLabel: nodeDescription(n), })),
    edges: graph.edges.map((e) => ({ id: e.id, type: "relationship", source: e.from, target: e.to,
      data: { edge: e, chosen: chosen === e.id }, selectable: false, focusable: true,
      ariaLabel: `${e.label}, ${e.kind === "span_parent" ? "reported by integration" : e.evidence}` })),
  };
}
