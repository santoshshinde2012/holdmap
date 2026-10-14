<script lang="ts">
  // Interactive service mesh: clusters as hulls, directed dependency edges with live-traffic
  // animation, hover highlighting of the dependency chain, click-to-select (synced with the
  // details pane), zoom/fit controls, an on-demand minimap and a layered ↔ force layout toggle.
  import { SvelteFlow, Background, Controls, Panel, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { setContext } from "svelte";
  import ServiceNode from "./graph/ServiceNode.svelte";
  import ClusterNode from "./graph/ClusterNode.svelte";
  import TrafficEdge from "./graph/TrafficEdge.svelte";
  import FitOnChange from "./graph/FitOnChange.svelte";
  import AutoMiniMap from "./graph/AutoMiniMap.svelte";
  import Icon from "./Icon.svelte";
  import type { Graph, GraphNode } from "../lib/types";
  import { FIT_PADDING, FORCE_NODE_LIMIT, layout, layoutModeFor, toFlow, type FlowEdge, type FlowNode, type LayoutMode, type Positions } from "../lib/graph";
  import { prefGet, prefGetBoolean, prefGetChoice, prefSet } from "../lib/prefs";

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

  let mode = $state<LayoutMode>(prefGetChoice("layout", ["layered", "force"], "layered"));
  let animate = $state(prefGet("animate") !== "0");
  let external = $state(prefGet("external") !== "0");
  let widgets = $state(prefGetBoolean("widgets", true));
  let hover = $state<string | null>(null);
  $effect(() => prefSet("layout", mode));
  $effect(() => prefSet("animate", animate ? "1" : "0"));
  $effect(() => prefSet("external", external ? "1" : "0"));
  $effect(() => prefSet("widgets", widgets ? "1" : "0"));

  setContext("graph-actions", { stopCluster: (name: string) => onstopcluster(name) });

  const nodeTypes: NodeTypes = { service: ServiceNode, cluster: ClusterNode } as unknown as NodeTypes;
  const edgeTypes: EdgeTypes = { traffic: TrafficEdge } as unknown as EdgeTypes;

  const shown = $derived.by<Graph | null>(() => {
    if (!graph) return null;
    if (external && graph.nodes.some((n) => n.kind === "external")) return graph;
    const nodes = graph.nodes.filter((n) => n.kind !== "external");
    const ids = new Set(nodes.map((n) => n.id));
    return { ...graph, nodes, edges: graph.edges.filter((e) => ids.has(e.from) && ids.has(e.to)) };
  });

  const forceLimited = $derived((shown?.nodes.length ?? 0) > FORCE_NODE_LIMIT);
  const effectiveMode = $derived(layoutModeFor(shown?.nodes.length ?? 0, mode));
  // Re-layout only when the structure changes, so 3-second refreshes don't make nodes jump.
  const signature = $derived(
    shown ? `${effectiveMode}|${shown.nodes.map((n) => `${n.id}@${n.cluster ?? ""}`).join(",")}|${shown.edges.map((e) => e.id).join(",")}` : ""
  );
  // Memoised layout keyed by structure; plain (non-reactive) cache on purpose.
  const cache: { sig: string; pos: Positions } = { sig: "", pos: new Map() };
  const positions = $derived.by(() => {
    if (!shown) return new Map() as Positions;
    if (signature !== cache.sig) {
      cache.pos = layout(shown, effectiveMode);
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
  const hasRemote = $derived(graph?.nodes.some((n) => n.kind === "external") ?? false);
  const showExternal = $derived(external && hasRemote);
  const sparse = $derived(!!shown && shown.nodes.length > 0 && shown.edges.length === 0);
  const empty = $derived(!shown || shown.nodes.length === 0);
</script>

<div class="graph" class:reduced>
  {#if empty}
    <div class="empty">
      <span class="art" aria-hidden="true"><Icon name="graph" size={22} /></span>
      {#if ontoggleall && !all}
        <p>No connected project services</p>
        <span>The graph starts with your dev servers and anything they talk to. Turn on All services to map databases, apps and the rest of this machine.</span>
        <button type="button" class="cta" onclick={ontoggleall}>Show all services</button>
      {:else}
        <p>No services to map yet</p>
        <span>Start a dev server — connected services, clusters and dependencies show up here.</span>
      {/if}
    </div>
  {:else}
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      {edgeTypes}
      colorMode={dark ? "dark" : "light"}
      fitView
      fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.2 }}
      minZoom={0.2}
      maxZoom={2}
      nodesDraggable={false}
      nodesConnectable={false}
      elementsSelectable={true}
      onlyRenderVisibleElements={(shown?.nodes.length ?? 0) > 120}
      proOptions={{ hideAttribution: true }}
      onnodeclick={({ node }) => { if (node.type === "service") { lastSel = node.id; onselect((node.data as FlowNode["data"]).node ?? null); } }}
      onnodepointerenter={({ node }) => { if (node.type === "service") hover = node.id; }}
      onnodepointerleave={() => (hover = null)}
      onpaneclick={() => onselect(null)}
    >
      <FitOnChange token={signature} {focus} {reduced} />
      <Background gap={22} size={1.2} />
      <Controls showLock={false} position="bottom-left" fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.2, duration: reduced ? 0 : 320 }} />
      <AutoMiniMap {dark} />
      <Panel position="top-left">
        <div class="chrome">
          <div class="bar" role="toolbar" aria-label="Graph view options">
            <div class="seg" role="radiogroup" aria-label="Layout">
              <button role="radio" aria-checked={effectiveMode === "layered"} class:on={effectiveMode === "layered"} onclick={() => (mode = "layered")} title="Layered: clients on the left, data stores on the right"><Icon name="layers" size={12} />Layered</button>
              <button role="radio" aria-checked={effectiveMode === "force"} class:on={effectiveMode === "force"} disabled={forceLimited} onclick={() => (mode = "force")} title={forceLimited ? `Layered keeps large graphs responsive. Force is available for up to ${FORCE_NODE_LIMIT} services.` : "Force-directed: tightly connected services cluster together"}><Icon name="radar" size={12} />Force</button>
            </div>
            <button class="tog" aria-pressed={animate && !reduced} class:on={animate && !reduced} disabled={reduced} onclick={() => (animate = !animate)} title={reduced ? "Reduced motion is on in your system settings" : "Animate live connections"}><Icon name="zap" size={12} />Traffic</button>
            <span class="sep" aria-hidden="true"></span>
            {#if ontoggleall}<button class="tog" aria-pressed={all} class:on={all} onclick={ontoggleall} title="Include databases, apps and system services that aren't connected to your dev servers"><Icon name="layers" size={12} />All services</button>{/if}
            <button class="tog" aria-pressed={showExternal} class:on={showExternal} disabled={!hasRemote} onclick={() => (external = !external)} title={hasRemote ? "Show outbound connections to remote hosts" : "No outbound connections to remote hosts right now"}><Icon name="globe" size={12} />External</button>
            <span class="sep" aria-hidden="true"></span>
            <button class="tog" aria-pressed={widgets} class:on={widgets} onclick={() => (widgets = !widgets)} title="Show or hide the legend and counts"><Icon name="layers" size={12} />Legend</button>
          </div>
          {#if widgets && stats}
            <div class="stats" aria-live="polite">
              <span><b>{stats.n}</b> services</span><span><b>{stats.e}</b> links</span><span class="opt"><b>{stats.c}</b> clusters</span><span class="opt"><b>{stats.conns}</b> connections</span>
            </div>
          {/if}
          {#if widgets}
            <div class="legend" aria-hidden="true">
              {#each legend as [k, l]}<span class="lg lg-{k}"><i></i>{l}</span>{/each}
              <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" /></svg>uses</span>
              <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>remote</span>
            </div>
          {/if}
        </div>
      </Panel>
    </SvelteFlow>
    {#if sparse}
      <div class="hint" role="status">
        <Icon name="info" size={14} />
        <div>
          {#if ontoggleall && !all}
            <p>These services aren't connected to each other.</p>
            <button type="button" class="cta" onclick={ontoggleall}>Show all services</button>
          {:else}
            <p>No connections between these services yet. Links appear when they talk to each other.</p>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .graph { position: relative; height: 100%; min-height: 0; background: var(--bg); --edge: var(--border-strong); container-type: inline-size; }
  .graph :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-node-border-radius: 14px; --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .graph :global(.svelte-flow__node-service), .graph :global(.svelte-flow__node-cluster) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; }
  .graph :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  /* Shown only while part of the graph is off-screen; translucent until you reach for it. */
  .graph :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); opacity: 0.72; transition: opacity var(--dur-2) var(--ease); }
  .graph :global(.svelte-flow__minimap:hover), .graph :global(.svelte-flow__minimap:focus-within) { opacity: 1; }
  .graph :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .chrome { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; max-width: min(100%, 920px); }
  .bar { display: flex; gap: 6px; align-items: center; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: 11px; box-shadow: var(--shadow-sm); }
  .sep { width: 1px; height: 16px; background: var(--border); margin: 0 2px; flex: none; }
  .seg { display: inline-flex; padding: 2px; background: var(--surface-2); border-radius: 8px; }
  .seg button, .tog { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border: 0; border-radius: 6px; background: transparent; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm), 0 0 0 1px var(--border); }
  .tog.on { color: var(--accent); background: var(--accent-soft); }
  .tog:disabled, .seg button:disabled { opacity: 0.5; cursor: not-allowed; }
  .stats { display: flex; gap: 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; justify-content: flex-end; }
  .stats b { color: var(--text); }
  .legend { display: flex; gap: 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lg { display: inline-flex; align-items: center; gap: 5px; }
  .lg i { width: 10px; height: 10px; border-radius: 3px; border: 1.5px solid var(--border-strong); background: var(--surface); }
  .lg-service i { border-left: 3px solid var(--tone-green); }
  .lg-client i { border-style: dashed; }
  .lg-external i { border-style: dashed; background: var(--surface-2); }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 8px; color: var(--muted); text-align: center; padding: 0 24px; }
  .empty .art { width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--surface-3); color: var(--muted); }
  .empty p { margin: 8px 0 0; color: var(--text); font-size: var(--fs-title); line-height: var(--lh-title); letter-spacing: var(--ls-title); font-weight: var(--fw-semibold); }
  .empty span { font-size: var(--fs-body); line-height: var(--lh-body); max-width: 360px; text-wrap: pretty; }
  .cta { display: inline-flex; align-items: center; height: var(--h-sm); padding: 0 12px; margin-top: 4px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); cursor: pointer; box-shadow: var(--shadow-sm); }
  .cta:hover { border-color: var(--accent); color: var(--accent); }
  .hint { position: absolute; left: 50%; bottom: 28px; transform: translateX(-50%); z-index: 5; display: flex; gap: 10px; align-items: flex-start; max-width: min(420px, calc(100% - 120px)); padding: 10px 12px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; box-shadow: var(--shadow); color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .hint :global(svg) { flex: none; margin-top: 2px; color: var(--accent); }
  .hint p { margin: 0; text-wrap: pretty; }
  .hint .cta { margin-top: 8px; }
  /* Keep the stats clear of the toolbar when the drawer leaves the canvas narrow. */
  @container (max-width: 820px) { .stats .opt { display: none; } }
  @container (max-width: 640px) { .legend { display: none; } }
  .reduced :global(*) { animation: none !important; }
</style>
