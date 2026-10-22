<script lang="ts">
  import StatStrip, { type Stat } from "./ui/StatStrip.svelte";
  import { SvelteFlow, Background, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { setContext, tick, untrack } from "svelte";
  import ServiceNode from "./graph/ServiceNode.svelte";
  import ClusterNode from "./graph/ClusterNode.svelte";
  import TrafficEdge from "./graph/TrafficEdge.svelte";
  import GraphViewport from "./graph/GraphViewport.svelte";
  import AutoMiniMap from "./graph/AutoMiniMap.svelte";
  import Icon from "./Icon.svelte";
  import { ActionDock, ActionPill, IconButton } from "./ui";
  import type { Graph, GraphNode } from "../lib/types";
  import { FORCE_NODE_LIMIT, graphForQuery, graphNodeMatches, layout, layoutModeFor, toFlow, type FlowEdge, type FlowNode, type LayoutMode, type Point, type Positions } from "../lib/graph";
  import { prefGet, prefGetBoolean, prefGetChoice, prefSet } from "../lib/prefs";
  import { humanBytes } from "../lib/format";
  import { isComposingKey } from "../lib/keyboard";

  let { graph, selectedNode, dark, reduced, onselect, onstopcluster, all = false, ontoggleall, query = "", onclearquery, onvisiblechange, loading = false }: {
    all?: boolean; ontoggleall?: () => void;
    graph: Graph | null; selectedNode: string | null; dark: boolean; reduced: boolean;
    onselect: (n: GraphNode | null) => void; onstopcluster: (name: string) => void;
    query?: string; onclearquery?: () => void;
    onvisiblechange?: (ids: string[]) => void;
    loading?: boolean;
  } = $props();

  let mode = $state<LayoutMode>(prefGetChoice("layout", ["layered", "force"], "layered"));
  let animate = $state(prefGet("animate") !== "0");
  let external = $state(prefGet("external") !== "0");
  let widgets = $state(prefGetBoolean("widgets", true));
  let hover = $state<string | null>(null);
  let optionsOpen = $state(false);
  let optionsTrigger: HTMLButtonElement | undefined = $state();
  let optionsPanel: HTMLDivElement | undefined = $state();
  let width = $state(0), height = $state(0);
  let focusRequest = $state<{ id: string; version: number } | null>(null);
  $effect(() => prefSet("layout", mode));
  $effect(() => prefSet("animate", animate ? "1" : "0"));
  $effect(() => prefSet("external", external ? "1" : "0"));
  $effect(() => prefSet("widgets", widgets ? "1" : "0"));

  function requestFocus(id: string) { focusRequest = { id, version: (focusRequest?.version ?? 0) + 1 }; }
  setContext("graph-actions", { stopCluster: (name: string) => onstopcluster(name) });
  setContext("service-graph-actions", {
    select: (node: GraphNode) => { requestFocus(node.id); onselect(node); },
    focus: requestFocus,
  });
  const nodeTypes = { service: ServiceNode, cluster: ClusterNode } as unknown as NodeTypes;
  const edgeTypes = { traffic: TrafficEdge } as unknown as EdgeTypes;
  const scoped = $derived.by<Graph | null>(() => {
    if (!graph || external) return graph;
    const nodes = graph.nodes.filter((n) => n.kind !== "external");
    const ids = new Set(nodes.map((n) => n.id));
    return { ...graph, nodes, edges: graph.edges.filter((e) => ids.has(e.from) && ids.has(e.to)) };
  });
  const shown = $derived(graphForQuery(scoped, query));
  const graphStats = $derived<Stat[]>([
    { id: "services", label: "Services", value: shown ? shown.nodes.filter((n) => n.kind !== "external").length : null },
    { id: "links", label: "Links", value: shown ? shown.edges.length : null },
    { id: "groups", label: "Groups", value: shown ? shown.clusters.length : null, meta: "Compose, k8s, workspaces" },
    { id: "remote", label: "Remote", value: shown ? shown.edges.filter((e) => e.kind === "outbound").length : null, meta: "outbound connections" },
  ]);
  const matches = $derived(shown?.nodes.filter((n) => graphNodeMatches(n, query)).length ?? 0);
  let visibleSignature: string | null = null;
  $effect(() => {
    const ids = shown?.nodes.filter((node) => graphNodeMatches(node, query)).map((node) => node.id) ?? [];
    const key = JSON.stringify(ids);
    if (key === visibleSignature) return;
    visibleSignature = key;
    untrack(() => onvisiblechange?.(ids));
  });
  const forceLimited = $derived((shown?.nodes.length ?? 0) > FORCE_NODE_LIMIT);
  const effectiveMode = $derived(layoutModeFor(shown?.nodes.length ?? 0, mode));
  const signature = $derived(shown ? effectiveMode + "|" +
    shown.nodes.map((n) => n.id + "@" + (n.cluster ?? "")).sort().join(",") + "|" +
    shown.edges.map((e) => e.id + "@" + e.from + ">" + e.to).sort().join(",") + "|" +
    shown.clusters.map((c) => c.id + ":" + [...c.nodes].sort().join(",")).sort().join(";") : "");
  const cache: { sig: string; pos: Positions } = { sig: "", pos: new Map() };
  const positions = $derived.by(() => {
    if (!shown) return new Map() as Positions;
    if (signature !== cache.sig) { cache.pos = layout(shown, effectiveMode); cache.sig = signature; }
    return cache.pos;
  });
  const flow = $derived.by(() => {
    if (!shown) return { nodes: [] as FlowNode[], edges: [] as FlowEdge[] };
    return toFlow(shown, positions, { focus: hover ?? selectedNode, selectedId: selectedNode, animate: animate && !reduced });
  });
  let nodes = $state.raw<FlowNode[]>([]), edges = $state.raw<FlowEdge[]>([]);
  $effect(() => {
    nodes = flow.nodes.map((node) => ({ ...node, focusable: false, selectable: false }));
    edges = flow.edges;
  });
  $effect(() => { if (hover && !shown?.nodes.some((node) => node.id === hover)) hover = null; });
  $effect(() => {
    if (graph && selectedNode && !shown?.nodes.some((node) => node.id === selectedNode)) {
      untrack(() => onselect(null));
    }
  });
  $effect(() => {
    const id = selectedNode;
    untrack(() => { if (id && id !== focusRequest?.id) requestFocus(id); });
  });
  const focusCache: { key: string; value: Point | null } = { key: "", value: null };
  const focus = $derived.by(() => {
    const p = focusRequest ? positions.get(focusRequest.id) : null;
    const key = p ? focusRequest!.id + ":" + focusRequest!.version + ":" + p.x + ":" + p.y : "";
    if (key !== focusCache.key) { focusCache.key = key; focusCache.value = p ? { ...p } : null; }
    return focusCache.value;
  });
  const anchor = $derived.by(() => {
    const node = shown?.nodes.find((n) => n.is_dev) ?? shown?.nodes[0];
    return node ? positions.get(node.id) ?? null : null;
  });
  const viewKey = $derived(effectiveMode + "|" + query.trim() + "|" + all + "|" + external);
  const hasRemote = $derived(graph?.nodes.some((n) => n.kind === "external") ?? false);
  const empty = $derived(!shown?.nodes.length);
  const peer = $derived(shown?.nodes.find((n) => n.id === selectedNode && n.ports.length === 0) ?? null);
  const peerLinks = $derived(peer ? shown?.edges.filter((edge) => edge.from === peer.id || edge.to === peer.id) ?? [] : []);
  const legend = [["service", "Service"], ["client", "Client"], ["external", "External host"]];

  function closeOptions(restore = false) { optionsOpen = false; if (restore) optionsTrigger?.focus(); }
  function outside(event: PointerEvent) {
    if (optionsOpen && !optionsPanel?.contains(event.target as Node) && !optionsTrigger?.contains(event.target as Node)) closeOptions();
  }
  function optionsKey(event: KeyboardEvent) {
    if (!optionsOpen || isComposingKey(event) || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeOptions(true); }
  }
  $effect(() => {
    if (!optionsOpen) return;
    // Pointer activation need not focus a button in WebKit. Dismiss the disclosure before
    // the page's Escape action, while allowing an overlaid modal to own its keyboard.
    const key = (event: KeyboardEvent) => {
      if (event.target instanceof Element && event.target.closest('[aria-modal="true"]')) return;
      optionsKey(event);
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });
  function leaveOptions(event: FocusEvent) {
    if (!optionsPanel?.contains(event.relatedTarget as Node) && event.relatedTarget !== optionsTrigger) closeOptions();
  }
  function closePeer() {
    const id = peer?.id;
    onselect(null);
    void tick().then(() => {
      if (selectedNode) return;
      const region = document.getElementById("service-graph");
      const card = [...region?.querySelectorAll<HTMLButtonElement>("button[data-service-id]") ?? []].find((button) => button.dataset.serviceId === id);
      (card ?? region)?.focus({ preventScroll: true });
    });
  }
</script>

<svelte:window onpointerdown={outside} />

<section class="graph" class:reduced id="service-graph" tabindex="-1" aria-label="Service graph">
  <header class="graph-head">
    <div class="heading">
      <span class="graph-mark" aria-hidden="true"><Icon name="graph" size={18} /></span>
      <div><h2>Service graph</h2><p class="stats">Who talks to whom on this computer{#if loading}<span class="updating" role="status">Updating…</span>{/if}</p></div>
    </div>
    <div class="primary-controls" role="toolbar" aria-label="Graph view options">
      {#if ontoggleall}
        <ActionPill size="compact" toggle icon="layers" label="All services" pressed={all} title="Include local apps and system services alongside your development services" onclick={ontoggleall} />
      {/if}
      <ActionDock hinge={false} animate={false} size="compact" label="Layout" value={effectiveMode}
        options={[{ value: "layered", label: "Layered", icon: "layers", title: "Arrange services from clients to dependencies" },
          { value: "force", label: "Force", icon: "radar", title: forceLimited ? "Force is available for up to " + FORCE_NODE_LIMIT + " services. Layered keeps larger graphs responsive." : "Group connected services, with disconnected groups packed nearby", disabled: forceLimited }]}
        onchange={(v) => (mode = v)} />
      <div class="options-wrap" onfocusout={leaveOptions}>
        <button type="button" class="options-trigger" bind:this={optionsTrigger} aria-label="Graph options" aria-expanded={optionsOpen} aria-controls={optionsOpen ? "graph-display-options" : undefined}
          onclick={() => (optionsOpen = !optionsOpen)} onkeydown={optionsKey}><Icon name="sliders" size={14} /><span>Options</span><Icon name="chevron-down" size={12} /></button>
        {#if optionsOpen}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions (Escape dismisses the nonmodal disclosure from its child controls.) -->
          <div class="options" role="region" aria-label="Graph display options" id="graph-display-options" tabindex="-1" bind:this={optionsPanel} onkeydown={optionsKey}>
            <div class="options-heading"><strong>Display options</strong><IconButton icon="x" size="sm" label="Close graph options" onclick={() => closeOptions(true)} /></div>
            <div class="option"><div><strong>Traffic</strong><p>{reduced ? "Off while reduced motion is enabled." : "Animate observed connections."}</p></div><ActionPill size="compact" toggle label="Traffic" pressed={animate && !reduced} disabled={reduced} onclick={() => (animate = !animate)} /></div>
            <div class="option"><div><strong>External hosts</strong><p>{hasRemote ? "Include observed remote endpoints." : "No remote endpoints in this scan."}</p></div><ActionPill size="compact" toggle label="External" pressed={external && hasRemote} disabled={!hasRemote} title={hasRemote ? "Show outbound connections to remote hosts" : "No outbound connections to remote hosts right now"} onclick={() => (external = !external)} /></div>
            <div class="option"><div><strong>Legend</strong><p>Show the node and connection key.</p></div><ActionPill size="compact" toggle label="Legend" pressed={widgets} onclick={() => (widgets = !widgets)} /></div>
          </div>
        {/if}
      </div>
    </div>
  </header>
  <div class="graph-summary"><StatStrip compact label="Graph summary" stats={graphStats} /></div>
  {#if query.trim()}
    <div class="search-context" role="status">
      <Icon name="search" size={14} /><span><strong>{matches}</strong> {matches === 1 ? "match" : "matches"}{#if (shown?.nodes.length ?? 0) > matches} · {(shown?.nodes.length ?? 0) - matches} connected neighbours{/if}</span>
      {#if onclearquery}<button type="button" onclick={onclearquery}>Clear search</button>{/if}
    </div>
  {/if}
  {#if !empty && shown?.edges.length === 0}
    <div class="connection-note" role="status"><Icon name="info" size={14} /><span>No connections between these services yet. Pan to explore; select a card for details.</span></div>
  {/if}
  <div class="graph-body" bind:clientWidth={width} bind:clientHeight={height} aria-busy={loading || undefined}>
    {#if empty}
      <div class="empty" role="status">
        <span class="art" aria-hidden="true"><Icon name={query.trim() ? "search" : "graph"} size={24} /></span>
        {#if loading && !graph}
          <h3>Building the service graph…</h3><p>Reading observed services and connections.</p>
        {:else if query.trim()}
          <h3>No services match this search</h3><p>Try a service name, port or PID.</p>
          {#if onclearquery}<button type="button" class="cta" onclick={onclearquery}>Clear search</button>{/if}
        {:else if ontoggleall && !all}
          <h3>No connected project services</h3><p>The graph starts with development services and their peers. Include all services to explore the rest of this machine.</p>
          <button type="button" class="cta" onclick={ontoggleall}>Show all services</button>
        {:else}
          <h3>No services to map yet</h3><p>Start a service to see its ports and observed connections here.</p>
        {/if}
      </div>
    {:else}
      <SvelteFlow bind:nodes bind:edges {nodeTypes} {edgeTypes} colorMode={dark ? "dark" : "light"}
        minZoom={.08} maxZoom={2} nodesDraggable={false} nodesConnectable={false} nodesFocusable={false} edgesFocusable={false} elementsSelectable={false} deleteKey={null}
        onlyRenderVisibleElements={(shown?.nodes.length ?? 0) > 120} proOptions={{ hideAttribution: true }}
        ariaLabelConfig={{ "controls.ariaLabel": "Graph navigation", "controls.zoomIn.ariaLabel": "Zoom in on services", "controls.zoomOut.ariaLabel": "Zoom out of services" }}
        onnodepointerenter={({ node }) => { if (node.type === "service") hover = node.id; }} onnodepointerleave={() => (hover = null)}
        onpaneclick={() => onselect(null)}>
        <GraphViewport {viewKey} {focus} {anchor} {width} {height} {reduced} pending={loading} />
        <Background gap={24} size={1} />
        {#if !peer}<AutoMiniMap {dark} />{/if}
      </SvelteFlow>
      {#if peer}
        <aside class="peer-inspector" aria-label="Details for {peer.label}">
          <header><div><span class="eyebrow">{peer.kind === "external" ? "External host" : "Observed process"}</span><h3>{peer.label}</h3></div><IconButton icon="x" label="Close service details" onclick={closePeer} /></header>
          {#if peer.pids.length}<p class="mono">PID {peer.pids.join(", ")}</p>{/if}
          {#if peer.pids.length}<p>{peer.cpu_percent.toFixed(1)}% CPU · {humanBytes(peer.memory_bytes)} memory</p>{/if}
          <p>{peerLinks.length} observed {peerLinks.length === 1 ? "link" : "links"} · No listening ports reported</p>
          {#each peerLinks.flatMap((edge) => edge.remotes) as remote}<p class="mono remote">{remote}</p>{/each}
          <small>Connections show observed sockets, not messages or tool calls.</small>
        </aside>
      {/if}
    {/if}
  </div>
  {#if widgets && !empty}
    <div class="legend" aria-label="Graph legend">
      {#each legend as [kind, label]}<span class={"lg lg-" + kind}><i aria-hidden="true"></i>{label}</span>{/each}
      <span class="lg edge-key"><svg width="22" height="6" aria-hidden="true"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" /></svg>Uses</span>
      <span class="lg edge-key"><svg width="22" height="6" aria-hidden="true"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>Remote</span>
      <span class="guide">Select a card to inspect</span>
    </div>
  {/if}
</section>

<style>
  .graph-summary { padding: 0 16px 10px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .graph { display: flex; flex-direction: column; position: relative; height: 100%; min-height: 0; min-width: 0; background: var(--bg); container-type: inline-size; outline: none; }
  .graph:focus-visible { box-shadow: inset 0 0 0 2px var(--ring); }
  .graph-head { position: relative; display: flex; flex: none; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; padding: 12px 16px; background: var(--surface); border-bottom: 1px solid var(--border); z-index: 6; }
  .heading { display: flex; gap: 10px; align-items: center; min-width: 0; }
  .graph-mark { width: 34px; height: 34px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); color: var(--accent); }
  h2, h3, p { margin: 0; }
  h2 { color: var(--text); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .stats { display: flex; align-items: center; gap: 6px; margin-top: 2px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; }
  .stats span { color: var(--faint); }
  .primary-controls { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .options-wrap { display: inline-flex; }
  .options-trigger { display: inline-flex; align-items: center; gap: 6px; height: var(--h-sm); padding: 0 10px; border-radius: var(--r-md); border: 1px solid var(--border); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-body-sm); cursor: pointer; }
  .options-trigger:hover, .options-trigger[aria-expanded="true"] { background: var(--surface-2); color: var(--accent); border-color: var(--border-strong); }
  .options-trigger:focus-visible, .cta:focus-visible, .search-context button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .options { position: absolute; top: calc(100% + 6px); right: 12px; width: min(320px, calc(100% - 24px)); box-sizing: border-box; border: 1px solid var(--border-strong); border-radius: var(--r-lg); background: var(--surface); box-shadow: var(--shadow-md); padding: 12px; color: var(--text); }
  .options-heading { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 4px; font-size: var(--fs-body); }
  .option { display: flex; gap: 16px; justify-content: space-between; align-items: center; padding: 12px 0; border-top: 1px solid var(--border); }
  .option strong { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); }
  .option p { margin-top: 3px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .search-context { display: flex; flex: none; align-items: center; gap: 8px; padding: 8px 16px; border-bottom: 1px solid var(--border); background: var(--accent-soft); color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .search-context button { margin-left: auto; border: 0; background: transparent; padding: 0; color: var(--accent); font: inherit; cursor: pointer; }
  .graph-body { position: relative; flex: 1; min-height: 0; min-width: 0; }
  .graph :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-node-border-radius: 14px; --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .graph :global(.svelte-flow__node-service), .graph :global(.svelte-flow__node-cluster) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; }
  .graph :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  .graph :global(.svelte-flow__controls-button) { width: 34px; height: 34px; }
  .graph :global(.svelte-flow__controls-button.readable-zoom) { width: 54px; font: inherit; font-size: var(--fs-caption); font-variant-numeric: tabular-nums; border-right: 1px solid var(--border); }
  .graph :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); opacity: .85; }
  .graph :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .legend { display: flex; flex: none; gap: 14px; align-items: center; flex-wrap: wrap; padding: 8px 16px; border-top: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .lg { display: inline-flex; align-items: center; gap: 5px; }
  .lg i { width: 10px; height: 10px; border-radius: 3px; border: 1.5px solid var(--border-strong); background: var(--surface); }
  .lg-service i { border-left: 3px solid var(--tone-green); }
  .lg-client i { border-style: dashed; }
  .lg-external i { border-style: dashed; background: var(--surface-2); }
  .guide { margin-left: auto; color: var(--faint); }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 10px; color: var(--muted); text-align: center; padding: 20px; }
  .empty .art { width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--surface-3); color: var(--muted); }
  .empty h3 { color: var(--text); font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); }
  .empty p { font-size: var(--fs-body); line-height: var(--lh-body); max-width: 360px; text-wrap: pretty; }
  .cta { height: var(--h-sm); padding: 0 12px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .connection-note { flex: none; display: flex; align-items: center; gap: 8px; padding: 8px 16px; border-bottom: 1px solid var(--border); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .peer-inspector { position: absolute; z-index: 5; bottom: 16px; right: 16px; width: min(320px, calc(100% - 32px)); max-height: calc(100% - 32px); overflow-y: auto; overscroll-behavior: contain; box-sizing: border-box; padding: 16px; border: 1px solid var(--border-strong); border-radius: var(--r-lg); background: var(--surface); color: var(--text-2); box-shadow: var(--shadow-md); display: grid; gap: 10px; font-size: var(--fs-body-sm); }
  .peer-inspector header { display: flex; gap: 12px; justify-content: space-between; align-items: center; }
  .peer-inspector h3 { color: var(--text); font-size: var(--fs-body); line-height: var(--lh-body); overflow-wrap: anywhere; }
  .eyebrow { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .peer-inspector small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .peer-inspector .remote { overflow-wrap: anywhere; }
  @container (max-width: 560px) {
    .graph-head { padding: 10px 12px; gap: 10px; }
    .primary-controls { gap: 6px; width: 100%; justify-content: space-between; }
    .guide, .edge-key { display: none; }
    .legend { padding: 8px 12px; gap: 12px; }
    .search-context { padding: 8px 12px; }
    .graph :global(.svelte-flow__minimap) { display: none; }
    .peer-inspector { bottom: 66px; max-height: calc(100% - 82px); }
  }
  .reduced :global(*) { animation: none !important; }
</style>
