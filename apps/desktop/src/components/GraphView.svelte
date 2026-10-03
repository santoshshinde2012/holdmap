<script lang="ts">
  // Interactive service mesh: clusters as hulls, directed dependency edges with live-traffic
  // animation, hover highlighting of the dependency chain, click-to-select (synced with the
  // details pane), zoom/fit controls, minimap and a layered ↔ force layout toggle.
  import { SvelteFlow, Background, Controls, MiniMap, Panel, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { setContext } from "svelte";
  import ServiceNode from "./graph/ServiceNode.svelte";
  import ClusterNode from "./graph/ClusterNode.svelte";
  import TrafficEdge from "./graph/TrafficEdge.svelte";
  import FitOnChange from "./graph/FitOnChange.svelte";
  import Icon from "./Icon.svelte";
  import type { Graph, GraphNode } from "../lib/types";
  import { layout, toFlow, type FlowEdge, type FlowNode, type LayoutMode, type Positions } from "../lib/graph";

  let {
    graph,
    selectedNode,
    dark,
    reduced,
    onselect,
    onstopcluster,
    all = false,
    ontoggleall,
  }: {
    all?: boolean;
    ontoggleall?: () => void;
    graph: Graph | null;
    selectedNode: string | null;
    dark: boolean;
    reduced: boolean;
    onselect: (n: GraphNode | null) => void;
    onstopcluster: (name: string) => void;
  } = $props();

  const store = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
  const save = (k: string, v: string) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } };
  let mode = $state<LayoutMode>((store("pw.layout") as LayoutMode) ?? "layered");
  let animate = $state(store("pw.animate") !== "0");
  let external = $state(store("pw.external") !== "0");
  let hover = $state<string | null>(null);
  $effect(() => save("pw.layout", mode));
  $effect(() => save("pw.animate", animate ? "1" : "0"));
  $effect(() => save("pw.external", external ? "1" : "0"));

  setContext("graph-actions", { stopCluster: (name: string) => onstopcluster(name) });

  const nodeTypes: NodeTypes = { service: ServiceNode, cluster: ClusterNode } as unknown as NodeTypes;
  const edgeTypes: EdgeTypes = { traffic: TrafficEdge } as unknown as EdgeTypes;

  const shown = $derived.by<Graph | null>(() => {
    if (!graph) return null;
    if (external) return graph;
    const nodes = graph.nodes.filter((n) => n.kind !== "external");
    const ids = new Set(nodes.map((n) => n.id));
    return { ...graph, nodes, edges: graph.edges.filter((e) => ids.has(e.from) && ids.has(e.to)) };
  });

  // Re-layout only when the structure changes, so 3-second refreshes don't make nodes jump.
  const signature = $derived(
    shown ? `${mode}|${shown.nodes.map((n) => `${n.id}@${n.cluster ?? ""}`).join(",")}|${shown.edges.map((e) => e.id).join(",")}` : ""
  );
  // Memoised layout keyed by structure; plain (non-reactive) cache on purpose.
  const cache: { sig: string; pos: Positions } = { sig: "", pos: new Map() };
  const positions = $derived.by(() => {
    if (!shown) return new Map() as Positions;
    if (signature !== cache.sig) {
      cache.pos = layout(shown, mode);
      cache.sig = signature;
    }
    return cache.pos;
  });
  const flow = $derived.by(() => {
    if (!shown) return { nodes: [] as FlowNode[], edges: [] as FlowEdge[] };
    return toFlow(shown, positions, { focus: hover ?? selectedNode, selectedId: selectedNode, animate: animate && !reduced });
  });
  let nodes = $state.raw<FlowNode[]>([]);
  let edges = $state.raw<FlowEdge[]>([]);
  $effect(() => { nodes = flow.nodes; edges = flow.edges; });

  // Centre on a node selected from elsewhere (list, keyboard) when it's off-screen-ish.
  let focus = $state<{ x: number; y: number } | null>(null);
  let lastSel: string | null = null;
  $effect(() => {
    const id = selectedNode;
    if (id === lastSel) return;
    lastSel = id;
    const p = id ? positions.get(id) : undefined;
    focus = p ? { x: p.x, y: p.y } : null;
  });

  const legend = [
    ["service", "Service"],
    ["client", "Client"],
    ["external", "External"],
  ];
  const stats = $derived(shown ? { n: shown.nodes.length, e: shown.edges.length, c: shown.clusters.length, conns: shown.stats.connections } : null);
</script>

<div class="graph" class:reduced>
  {#if !shown || shown.nodes.length === 0}
    <div class="empty">
      <Icon name="graph" size={28} />
      <p>No services to map yet.</p>
      <span>Start a dev server — connected services, clusters and dependencies show up here.</span>
    </div>
  {:else}
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      {edgeTypes}
      colorMode={dark ? "dark" : "light"}
      fitView
      minZoom={0.2}
      maxZoom={2}
      nodesDraggable={false}
      nodesConnectable={false}
      elementsSelectable={true}
      onlyRenderVisibleElements={shown.nodes.length > 120}
      proOptions={{ hideAttribution: true }}
      onnodeclick={({ node }) => { if (node.type === "service") { lastSel = node.id; onselect((node.data as FlowNode["data"]).node ?? null); } }}
      onnodepointerenter={({ node }) => { if (node.type === "service") hover = node.id; }}
      onnodepointerleave={() => (hover = null)}
      onpaneclick={() => onselect(null)}
    >
      <FitOnChange token={signature} {focus} {reduced} />
      <Background gap={22} size={1.2} />
      <Controls showLock={false} position="bottom-left" />
      <MiniMap
        position="bottom-right"
        pannable
        zoomable
        nodeColor={(n) => (n.type === "cluster" ? "transparent" : (n.data as FlowNode["data"]).node?.is_dev ? "var(--tone-green)" : "var(--border-strong)")}
        nodeStrokeColor={(n) => (n.type === "cluster" ? "var(--accent)" : "transparent")}
        maskColor={dark ? "rgb(0 0 0 / 0.55)" : "rgb(240 240 245 / 0.65)"}
        width={180}
        height={120}
      />
      <Panel position="top-left">
        <div class="bar" role="toolbar" aria-label="Graph view options">
          <div class="seg" role="radiogroup" aria-label="Layout">
            <button role="radio" aria-checked={mode === "layered"} class:on={mode === "layered"} onclick={() => (mode = "layered")} title="Layered: clients on the left, data stores on the right"><Icon name="layers" size={12} />Layered</button>
            <button role="radio" aria-checked={mode === "force"} class:on={mode === "force"} onclick={() => (mode = "force")} title="Force-directed: tightly connected services cluster together"><Icon name="radar" size={12} />Force</button>
          </div>
          <button class="tog" aria-pressed={animate && !reduced} class:on={animate && !reduced} disabled={reduced} onclick={() => (animate = !animate)} title={reduced ? "Reduced motion is on in your system settings" : "Animate live connections"}><Icon name="zap" size={12} />Traffic</button>
          {#if ontoggleall}<button class="tog" aria-pressed={all} class:on={all} onclick={ontoggleall} title="Include databases, apps and system services that aren't connected to your dev servers"><Icon name="layers" size={12} />All services</button>{/if}
          <button class="tog" aria-pressed={external} class:on={external} onclick={() => (external = !external)} title="Show outbound connections to remote hosts"><Icon name="globe" size={12} />External</button>
        </div>
      </Panel>
      <Panel position="top-right">
        {#if stats}
          <div class="stats" aria-live="polite">
            <span><b>{stats.n}</b> services</span><span><b>{stats.e}</b> links</span><span><b>{stats.c}</b> clusters</span><span><b>{stats.conns}</b> connections</span>
          </div>
        {/if}
        <div class="legend" aria-hidden="true">
          {#each legend as [k, l]}<span class="lg lg-{k}"><i></i>{l}</span>{/each}
          <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" /></svg>uses</span>
          <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>remote</span>
        </div>
      </Panel>
    </SvelteFlow>
  {/if}
</div>

<style>
  .graph { position: relative; height: 100%; min-height: 0; background: var(--bg); --edge: var(--border-strong); }
  .graph :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-node-border-radius: 14px; --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .graph :global(.svelte-flow__node-service), .graph :global(.svelte-flow__node-cluster) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; }
  .graph :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  .graph :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); }
  .graph :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .bar { display: flex; gap: 6px; align-items: center; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: 11px; box-shadow: var(--shadow-sm); }
  .seg { display: inline-flex; padding: 2px; background: var(--surface-2); border-radius: 8px; }
  .seg button, .tog { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border: 0; border-radius: 6px; background: transparent; color: var(--muted); font-size: 11px; font-weight: 600; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm), 0 0 0 1px var(--border); }
  .tog.on { color: var(--accent); background: var(--accent-soft); }
  .tog:disabled { opacity: 0.5; cursor: not-allowed; }
  .stats { display: flex; gap: 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; justify-content: flex-end; }
  .stats b { color: var(--text); }
  .legend { display: flex; gap: 10px; margin-top: 6px; justify-content: flex-end; font-size: 10.5px; color: var(--muted); }
  .lg { display: inline-flex; align-items: center; gap: 5px; }
  .lg i { width: 10px; height: 10px; border-radius: 3px; border: 1.5px solid var(--border-strong); background: var(--surface); }
  .lg-service i { border-left: 3px solid var(--tone-green); }
  .lg-client i { border-style: dashed; }
  .lg-external i { border-style: dashed; background: var(--surface-2); }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 6px; color: var(--muted); text-align: center; }
  .empty p { margin: 6px 0 0; color: var(--text); font-weight: 600; }
  .empty span { font-size: var(--fs-sm); max-width: 340px; }
  .reduced :global(*) { animation: none !important; }
</style>
