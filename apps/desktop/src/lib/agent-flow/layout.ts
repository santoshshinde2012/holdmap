import type { Edge, Node } from "@xyflow/svelte";
import type { VisualEdge, VisualGraph, VisualNode } from "./model";
import { nodeDescription } from "./build";

/** `dim`: something else is picked and this node is not next to it. */
export type FlowNodeData = { node: VisualNode; chosen: boolean; dim?: boolean };
/** `labelled`: whether the relationship's label is drawn. A busy graph names only the edges you pick or that touch the picked card, so labels never pile up. */
/** `near`: the edge touches the picked node; `dim`: something is picked and it does not. */
export type FlowEdgeData = { edge: VisualEdge; chosen: boolean; labelled: boolean; near?: boolean; dim?: boolean };
/** Above this many relationships the labels would collide at a readable zoom. */
export const LABEL_EDGE_LIMIT = 8;
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
  const crowded = graph.edges.length > LABEL_EDGE_LIMIT;
  // The picked node or edge and everything one hop away stay bright; the rest recede.
  const pickedEdge = graph.edges.find((e) => e.id === chosen);
  const lit = chosen ? new Set(pickedEdge ? [pickedEdge.from, pickedEdge.to] : [chosen, ...graph.edges.flatMap((e) => (e.from === chosen ? [e.to] : e.to === chosen ? [e.from] : []))]) : null;
  return {
    nodes: graph.nodes.map((n) => ({ id: n.id, type: "activity", position: positions.get(n.id) ?? { x: 0, y: 0 }, width: 244, height: 78,
      data: { node: n, chosen: chosen === n.id, dim: !!lit && !lit.has(n.id) }, draggable: false, connectable: false,
      ariaLabel: nodeDescription(n), })),
    edges: graph.edges.map((e) => {
      const near = chosen === e.from || chosen === e.to, on = near || chosen === e.id;
      return { id: e.id, type: "relationship", source: e.from, target: e.to,
      data: { edge: e, chosen: chosen === e.id, labelled: !crowded || on, near, dim: !!lit && !on }, selectable: false, focusable: true,
      ariaLabel: `${e.label}, ${e.kind === "span_parent" ? "reported by integration" : e.evidence}` };
    }),
  };
}
