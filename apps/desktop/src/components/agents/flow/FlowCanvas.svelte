<script lang="ts">
  import { SvelteFlow, Background, Controls, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import FlowNode from "./FlowNode.svelte";
  import FlowEdge from "./FlowEdge.svelte";
  import FlowViewport from "./FlowViewport.svelte";
  import AutoMiniMap from "../../graph/AutoMiniMap.svelte";
  import { FIT_PADDING } from "../../../lib/flow";
  import { graphSignature } from "../../../lib/agent-flow/build";
  import { layoutGraph, toFlow, type FlowNode as FNode, type FlowEdge as FEdge } from "../../../lib/agent-flow/layout";
  import type { VisualGraph } from "../../../lib/agent-flow/model";
  let { graph, calls, chosen, dark, reduced, onselect }: { graph: VisualGraph; calls: boolean; chosen: string | null; dark: boolean; reduced: boolean; onselect: (id: string | null) => void } = $props();
  const nodeTypes = { activity: FlowNode } as unknown as NodeTypes;
  const edgeTypes = { relationship: FlowEdge } as unknown as EdgeTypes;
  const signature = $derived(graphSignature(graph));
  const cache = { signature: "", calls: false, positions: new Map<string, { x: number; y: number }>() };
  const positions = $derived.by(() => {
    if (cache.signature !== signature || cache.calls !== calls) {
      cache.positions = layoutGraph(graph, calls); cache.signature = signature; cache.calls = calls;
    }
    return cache.positions;
  });
  const focus = $derived.by(() => { const point = chosen ? positions.get(chosen) : null; return point ? { x: point.x + 122, y: point.y + 39 } : null; });
  const flow = $derived(toFlow(graph, positions, chosen));
  let nodes = $state.raw<FNode[]>([]);
  let edges = $state.raw<FEdge[]>([]);
  $effect(() => { nodes = flow.nodes; edges = flow.edges; });
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" && event.key !== " ") return;
    const target = (event.target as HTMLElement)?.closest?.(".svelte-flow__node, .svelte-flow__edge");
    const id = target?.getAttribute("data-id");
    if (id) { event.preventDefault(); event.stopPropagation(); onselect(id); }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="canvas" {onkeydown} aria-label={calls ? "Reported call relationships" : "Observed process connections"}>
  <SvelteFlow bind:nodes bind:edges {nodeTypes} {edgeTypes} colorMode={dark ? "dark" : "light"}
    fitView fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.15 }} minZoom={0.12} maxZoom={1.8}
    nodesDraggable={false} nodesConnectable={false} elementsSelectable={false} deleteKey={null}
    onlyRenderVisibleElements={nodes.length > 120} proOptions={{ hideAttribution: true }}
    onnodeclick={({ node }) => onselect(node.id)} onedgeclick={({ edge }) => onselect(edge.id)} onpaneclick={() => onselect(null)}
    ariaLabelConfig={{ "controls.fitView.ariaLabel": "Fit all relationships in view" }}>
    <Background gap={22} size={1} />
    <Controls showLock={false} position="bottom-left" fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.15, duration: reduced ? 0 : 300 }} />
    <AutoMiniMap {dark} />
    <FlowViewport token={signature} {focus} {reduced} />
  </SvelteFlow>
  <div class="map-note">{calls ? "Reported parent relationships" : "Process ownership and live sockets"} · Drag to explore</div>
</div>

<style>
  .canvas { position: relative; min-width: 0; min-height: 0; height: 100%; overflow: hidden; background: var(--bg); }
  .canvas :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .canvas :global(.svelte-flow__node-activity) { padding: 0; border: 0; background: transparent; box-shadow: none; border-radius: 12px; }
  .canvas :global(.svelte-flow__node:focus-visible), .canvas :global(.svelte-flow__edge:focus-visible) { outline: 2px solid var(--ring); outline-offset: 3px; }
  .canvas :global(.svelte-flow__controls) { border: 1px solid var(--border); border-radius: 9px; overflow: hidden; box-shadow: var(--shadow-sm); }
  .map-note { position: absolute; top: 12px; left: 12px; max-width: calc(100% - 24px); pointer-events: none; padding: 6px 9px; border: 1px solid var(--border); background: var(--surface); border-radius: 7px; color: var(--muted); font-size: var(--fs-caption); box-shadow: var(--shadow-sm); }
</style>
