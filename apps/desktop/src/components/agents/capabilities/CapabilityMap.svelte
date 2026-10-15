<script lang="ts">
  import { SvelteFlow, Background, Controls, type Node, type Edge, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { untrack } from "svelte";
  import CapabilityNode from "./CapabilityNode.svelte";
  import CapabilityLane from "./CapabilityLane.svelte";
  import CapabilityEdge from "./CapabilityEdge.svelte";
  import AutoMiniMap from "../../graph/AutoMiniMap.svelte";
  import CapabilityViewport from "./CapabilityViewport.svelte";
  import Icon from "../../Icon.svelte";
  import { agentColor } from "../../../lib/agents";
  import { capabilityLabel, type CapabilityItem, type RunningMcpItem } from "../../../lib/agent-capabilities";
  import { buildCapabilityMap, type CapabilityGraphNode } from "../../../lib/agent-capability-map";
  import { FIT_PADDING } from "../../../lib/flow";
  import type { Agent } from "../../../lib/types";

  let { agents, items, running, agentIds, selectedAgent = null, selectedId = null, filtered = false, dark, reduced, onselect, onselectagent }: {
    agents: Agent[]; items: CapabilityItem[]; running: RunningMcpItem[];
    agentIds?: readonly string[];
    selectedAgent?: string | null; selectedId?: string | null; filtered?: boolean;
    dark: boolean; reduced: boolean; onselect: (id: string) => void; onselectagent: (id: string | null) => void;
  } = $props();
  let element: HTMLDivElement | undefined = $state();
  let width = $state(0), height = $state(0);
  let hover = $state<string | null>(null);
  let focusRequest = $state<{ id: string; version: number } | null>(null);
  const graph = $derived(buildCapabilityMap({ agents, items, running, selectedAgent, selectedId, filtered }));
  const nodeTypes = { capability: CapabilityNode, agent: CapabilityNode, header: CapabilityLane } as unknown as NodeTypes;
  const edgeTypes = { definition: CapabilityEdge } as unknown as EdgeTypes;
  function requestFocus(id: string) { focusRequest = { id, version: (focusRequest?.version ?? 0) + 1 }; }
  $effect(() => { const id = selectedId; untrack(() => { if (id && focusRequest?.id !== id) requestFocus(id); }); });
  const palette = $derived(new Map(agents.map((agent) => [agent.id, agentColor(null, agent.id, agentIds ?? agents.map((owner) => owner.id))])));
  const color = (node: CapabilityGraphNode) => node.kind === "running" ? "var(--muted)" : palette.get(node.ownerIds[0] ?? node.id) ?? "var(--accent)";
  $effect(() => { if (hover && !graph.nodes.some((node) => node.id === hover)) hover = null; });
  const relationship = $derived.by(() => {
    const id = hover && graph.nodes.some((node) => node.id === hover) ? hover : selectedId;
    if (!id || !graph.nodes.some((node) => node.id === id)) return null;
    const nodes = new Set([id]), edges = new Set<string>();
    for (const edge of graph.edges) if (edge.from === id || edge.to === id) { nodes.add(edge.from); nodes.add(edge.to); edges.add(edge.id); }
    return { nodes, edges };
  });
  function activate(node: CapabilityGraphNode) { if (node.kind === "agent") onselectagent(node.agent?.id ?? node.id); else onselect(node.id); }
  function accessibleName(node: CapabilityGraphNode): string {
    return node.item ? capabilityLabel(node.item) : node.runtime ? `Inspect running process ${node.label}, PID ${node.runtime.tool.pid}` : `Inspect capabilities for ${node.label}, PID ${node.agent?.pid ?? "unknown"}`;
  }
  const flowNodes = $derived<Node[]>([
    ...graph.lanes.map((lane) => ({ id: `capability-lane:${lane.id}`, type: "header", position: { x: lane.x, y: lane.y }, width: lane.width, height: lane.height, data: { lane }, draggable: false, connectable: false, selectable: false, focusable: false, zIndex: -1 })),
    ...graph.nodes.map((node) => ({ id: node.id, type: node.kind === "agent" ? "agent" : "capability", position: node.position, width: node.width, height: node.height, draggable: false, connectable: false, selectable: false, focusable: false, zIndex: 1,
      data: { node, color: color(node), label: accessibleName(node), selected: node.id === selectedId, dim: !!relationship && !relationship.nodes.has(node.id), highlighted: !!relationship && relationship.nodes.has(node.id),
        onactivate: () => activate(node), onfocus: (event: FocusEvent) => {
          // Pointer focus happens before mouseup: moving the viewport here would
          // move the pressed card away and cancel its native click.
          if ((event.currentTarget as HTMLElement | null)?.matches(":focus-visible")) requestFocus(node.id);
        } } })),
  ]);
  const flowEdges = $derived<Edge[]>(graph.edges.map((edge) => ({ id: edge.id, type: "definition", source: edge.from, target: edge.to, sourceHandle: edge.sourceHandle, selectable: false, focusable: false,
    data: { edge, color: palette.get(edge.from) ?? "var(--accent)", dim: !!relationship && !relationship.edges.has(edge.id), highlighted: !!relationship && relationship.edges.has(edge.id) } })));
  let nodes = $state.raw<Node[]>([]), edges = $state.raw<Edge[]>([]);
  $effect(() => { nodes = flowNodes; edges = flowEdges; });
  const focusCache: { key: string; value: { x: number; y: number } | null } = { key: "", value: null };
  const focus = $derived.by(() => {
    const node = graph.nodes.find((node) => node.id === focusRequest?.id);
    const key = node ? `${node.id}:${focusRequest?.version}:${node.position.x}:${node.position.y}:${node.width}:${node.height}` : "";
    if (key !== focusCache.key) { focusCache.key = key; focusCache.value = node ? { x: node.position.x + node.width / 2, y: node.position.y + node.height / 2 } : null; }
    return focusCache.value;
  });
  const hub = $derived.by(() => {
    const node = graph.nodes.find((node) => node.id === selectedId) ?? graph.nodes.find((node) => node.kind === "agent");
    return node ? { x: node.position.x + node.width / 2, y: node.position.y + node.height / 2 } : null;
  });
  const partial = $derived(graph.omitted.agents > 0 || graph.omitted.inventory > 0 || graph.omitted.running > 0 || graph.omitted.ownerEdges > 0);
  const token = $derived(`${graph.signature}:${Math.round(width)}:${Math.round(height)}`);
  export function focusNode(id: string): boolean {
    const target = [...element?.querySelectorAll<HTMLButtonElement>("button[data-capability-id]") ?? []].find((button) => button.dataset.capabilityId === id);
    if (!target) return false;
    target.focus({ preventScroll: true }); requestFocus(id); return true;
  }
</script>

<div class="capability-map" class:reduced bind:this={element} bind:clientWidth={width} bind:clientHeight={height} role="region" aria-label="Agent capability graph">
  <SvelteFlow bind:nodes bind:edges {nodeTypes} {edgeTypes} colorMode={dark ? "dark" : "light"}
    fitView fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.1 }} minZoom={.08} maxZoom={2}
    nodesDraggable={false} nodesConnectable={false} nodesFocusable={false} edgesFocusable={false} elementsSelectable={false} deleteKey={null}
    proOptions={{ hideAttribution: true }}
    onnodepointerenter={({ node }) => { if (node.type !== "header") hover = node.id; }} onnodepointerleave={() => { hover = null; }}
    ariaLabelConfig={{ "controls.fitView.ariaLabel": "Fit all capabilities in view", "controls.zoomIn.ariaLabel": "Zoom in on capabilities", "controls.zoomOut.ariaLabel": "Zoom out of capabilities" }}>
    <Background gap={22} size={1.2} />
    <Controls showLock={false} position="bottom-left" fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.1, duration: reduced ? 0 : 320 }} />
    <AutoMiniMap {dark} />
    <CapabilityViewport {token} {focus} {hub} {width} {height} {reduced} />
  </SvelteFlow>
  <div class="map-notes"><div class="map-hint"><Icon name="branch" size={12} /><span>Drag to explore · select a card for details</span></div>{#if partial}<div class="map-partial" role="status">Map shows {graph.shown.agents} of {graph.total.agents} agents, {graph.shown.inventory} of {graph.total.inventory} definitions and {graph.shown.running} of {graph.total.running} runtime candidates.{#if graph.omitted.ownerEdges} {graph.omitted.ownerEdges} source relationships not drawn.{/if}</div>{/if}</div>
</div>

<style>
  .capability-map { position: relative; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; background: var(--bg); }
  .capability-map :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .capability-map :global(.svelte-flow__node-capability), .capability-map :global(.svelte-flow__node-agent), .capability-map :global(.svelte-flow__node-header) { padding: 0; border: 0; background: transparent; box-shadow: none; border-radius: 15px; }
  .capability-map :global(.svelte-flow__node-header) { pointer-events: none; }
  .capability-map :global(.svelte-flow__controls) { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; box-shadow: var(--shadow-sm); }
  .capability-map :global(.svelte-flow__minimap) { border: 1px solid var(--border); border-radius: 12px; overflow: hidden; box-shadow: var(--shadow-sm); opacity: .78; }
  .capability-map :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .map-notes { position: absolute; top: 12px; left: 12px; display: grid; gap: 5px; justify-items: start; max-width: calc(100% - 24px); pointer-events: none; }
  .map-hint, .map-partial { box-sizing: border-box; padding: 6px 9px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); box-shadow: var(--shadow-sm); }
  .map-hint { display: flex; align-items: center; gap: 6px; }
  .map-partial { max-width: 480px; overflow-wrap: anywhere; }
  .reduced :global(*) { animation: none !important; transition: none !important; }
  @container (max-width: 560px) { .map-notes { top: 8px; left: 8px; } }
</style>
