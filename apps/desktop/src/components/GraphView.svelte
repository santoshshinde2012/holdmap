<script lang="ts">
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
  import type { Agent, Graph, GraphNode } from "../lib/types";
  import { agentForPids } from "../lib/agent-scope";
  import { FORCE_NODE_LIMIT, graphForQuery, graphNodeMatches, layout, layoutModeFor, toFlow, type FlowEdge, type FlowNode, type LayoutMode, type Point, type Positions } from "../lib/graph";
  import { prefGet, prefGetBoolean, prefGetChoice, prefSet } from "../lib/prefs";
  import { humanBytes } from "../lib/format";
  import { isComposingKey } from "../lib/keyboard";

  let { graph, selectedNode, dark, reduced, onselect, onstopcluster, all = false, ontoggleall, query = "", onclearquery, onvisiblechange, loading = false, agents = [], onopenagent }: {
    all?: boolean; ontoggleall?: () => void;
    graph: Graph | null; selectedNode: string | null; dark: boolean; reduced: boolean;
    onselect: (n: GraphNode | null) => void; onstopcluster: (name: string) => void;
    query?: string; onclearquery?: () => void;
    onvisiblechange?: (ids: string[]) => void;
    loading?: boolean;
    /** Running agents, so a peer process that belongs to one can open it in Agents. */
    agents?: Agent[];
    onopenagent?: (id: string) => void;
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
  /** A cluster whose members are highlighted from its header; independent of the selection. */
  let clusterFocus = $state<string | null>(null);
  setContext("graph-actions", {
    stopCluster: (name: string) => onstopcluster(name),
    focusCluster: (id: string) => { clusterFocus = clusterFocus === id ? null : id; },
  });
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
  const graphStats = $derived([
    { id: "services", label: "Services", value: shown ? shown.nodes.filter((n) => n.kind !== "external").length : 0, title: "Services and client processes" },
    { id: "links", label: "Links", value: shown ? shown.edges.length : 0, title: "Observed connections" },
    { id: "groups", label: "Groups", value: shown ? shown.clusters.length : 0, title: "Compose, k8s, workspaces" },
    { id: "remote", label: "Remote", value: shown ? shown.edges.filter((e) => e.kind === "outbound").length : 0, title: "Outbound connections" },
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
  const focusedCluster = $derived(clusterFocus ? shown?.clusters.find((c) => c.id === clusterFocus) ?? null : null);
  const flow = $derived.by(() => {
    if (!shown) return { nodes: [] as FlowNode[], edges: [] as FlowEdge[] };
    const cluster = focusedCluster;
    const base = toFlow(shown, positions, { focus: cluster ? null : hover ?? selectedNode, selectedId: selectedNode, animate: animate && !reduced });
    if (!cluster) return base;
    const members = new Set(cluster.nodes);
    return {
      nodes: base.nodes.map((n) => n.type === "cluster"
        ? { ...n, data: { ...n.data, dim: n.data.cluster?.id !== cluster.id, focused: n.data.cluster?.id === cluster.id } }
        : { ...n, data: { ...n.data, dim: !members.has(n.id), hl: members.has(n.id) } }),
      edges: base.edges.map((e) => {
        const inside = members.has(e.source) && members.has(e.target);
        return { ...e, data: { ...e.data!, dim: !inside, hl: inside } };
      }),
    };
  });
  // Selecting a service outside the highlighted cluster ends the highlight.
  $effect(() => {
    const id = selectedNode;
    untrack(() => { if (id && focusedCluster && !focusedCluster.nodes.includes(id)) clusterFocus = null; });
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
  // A lone "External" card with no service beside it is still an empty graph.
  const empty = $derived(!shown?.nodes.some((n) => n.kind !== "external"));
  const peer = $derived(shown?.nodes.find((n) => n.id === selectedNode && n.ports.length === 0) ?? null);
  const peerLinks = $derived(peer ? shown?.edges.filter((edge) => edge.from === peer.id || edge.to === peer.id) ?? [] : []);
  const peerNeighbors = $derived.by(() => {
    if (!peer || !shown) return [] as { node: GraphNode; direction: "uses" | "used by" }[];
    const seen = new Map<string, { node: GraphNode; direction: "uses" | "used by" }>();
    for (const edge of peerLinks) {
      const other = edge.from === peer.id ? edge.to : edge.from;
      const node = shown.nodes.find((n) => n.id === other);
      if (node && !seen.has(node.id)) seen.set(node.id, { node, direction: edge.from === peer.id ? "uses" : "used by" });
    }
    return [...seen.values()];
  });
  const peerRemotes = $derived([...new Set(peerLinks.flatMap((edge) => edge.remotes))]);
  const peerAgent = $derived(peer ? agentForPids(agents, peer.pids) : null);
  function selectNeighbor(node: GraphNode) { requestFocus(node.id); onselect(node); }
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

<section class="graph" class:reduced class:has-peer={!!peer} id="service-graph" tabindex="-1" aria-label="Service graph">
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
        <AutoMiniMap {dark} />
      </SvelteFlow>
    {/if}

    <!-- Floating chrome: frosted pills over the canvas, matching the Agents map. -->
    <div class="chrome-host">
      <div class="chrome">
        <div class="bar" role="toolbar" aria-label="Graph view options">
          {#if ontoggleall}
            <ActionPill size="compact" toggle icon="layers" label="All services" pressed={all} title="Include local apps and system services alongside your development services" onclick={ontoggleall} />
          {/if}
          <ActionDock hinge={false} animate={false} size="compact" label="Layout" value={effectiveMode}
            options={[{ value: "layered", label: "Layered", icon: "layers", title: "Arrange services from clients to dependencies" },
              { value: "force", label: "Force", icon: "radar", title: forceLimited ? "Force is available for up to " + FORCE_NODE_LIMIT + " services. Layered keeps larger graphs responsive." : "Group connected services, with disconnected groups packed nearby", disabled: forceLimited }]}
            onchange={(v) => (mode = v)} />
          <div class="options-wrap" onfocusout={leaveOptions}>
            <button type="button" class="options-trigger" bind:this={optionsTrigger} aria-label="Graph options" title="Graph options" aria-expanded={optionsOpen} aria-controls={optionsOpen ? "graph-display-options" : undefined}
              onclick={() => (optionsOpen = !optionsOpen)} onkeydown={optionsKey}><Icon name="sliders" size={13} /><span class="opt-label">Options</span><Icon name="chevron-down" size={11} /></button>
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
        {#if !empty}
          <div class="stats" role="group" aria-label="Graph summary">
            {#each graphStats as stat (stat.id)}<span class="stat stat-{stat.id}" title={stat.title}><b>{stat.value}</b> {stat.label}</span>{/each}
            {#if loading}<span class="updating" role="status">Updating…</span>{/if}
          </div>
        {/if}
        {#if widgets && !empty}
          <div class="legend" role="group" aria-label="Graph legend">
            {#each legend as [kind, label]}<span class={"lg lg-" + kind}><i aria-hidden="true"></i>{label}</span>{/each}
            <span class="lg edge-key"><svg width="18" height="6" aria-hidden="true"><line x1="0" y1="3" x2="18" y2="3" stroke="currentColor" stroke-width="1.6" /></svg>Uses</span>
            <span class="lg edge-key"><svg width="18" height="6" aria-hidden="true"><line x1="0" y1="3" x2="18" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>Remote</span>
          </div>
        {/if}
        {#if query.trim()}
          <div class="search-context" role="status">
            <Icon name="search" size={12} /><span><strong>{matches}</strong> {matches === 1 ? "match" : "matches"}{#if (shown?.nodes.length ?? 0) > matches} · {(shown?.nodes.length ?? 0) - matches} connected neighbours{/if}</span>
            {#if onclearquery}<button type="button" onclick={onclearquery}>Clear search</button>{/if}
          </div>
        {/if}
        {#if focusedCluster}
          <div class="cluster-focus" role="status">
            <Icon name="layers" size={12} /><span>Highlighting <strong>{focusedCluster.name}</strong> · {focusedCluster.nodes.length} {focusedCluster.nodes.length === 1 ? "service" : "services"}</span>
            <button type="button" onclick={() => (clusterFocus = null)}>Clear</button>
          </div>
        {/if}
        {#if !empty && shown?.edges.length === 0}
          <div class="connection-note" role="status"><Icon name="info" size={12} /><span>No connections between these services yet. Pan to explore; select a card for details.</span></div>
        {/if}
      </div>
    </div>
  </div>

  {#if peer}
    <aside class="peer-inspector" aria-label="Details for {peer.label}">
      <header>
        <span class="peer-tile" aria-hidden="true"><Icon name={peer.kind === "external" ? "globe" : "monitor"} size={16} /></span>
        <div class="peer-who"><span class="eyebrow">{peer.kind === "external" ? "External host" : "Observed process"}</span><h3>{peer.label}</h3></div>
        <IconButton icon="x" label="Close service details" onclick={closePeer} />
      </header>
      {#if peerAgent && onopenagent}
        <button type="button" class="peer-agent" onclick={() => onopenagent?.(peerAgent.id)}><Icon name="bot" size={13} /><span>Part of <b>{peerAgent.name}</b></span><span class="go">Open in Agents</span></button>
      {/if}
      <section class="peer-sec" aria-labelledby="peer-proc-head">
        <h4 id="peer-proc-head">Process</h4>
        {#if peer.pids.length}
          <p class="mono">PID {peer.pids.join(", ")}</p>
          <p>{peer.cpu_percent.toFixed(1)}% CPU · {humanBytes(peer.memory_bytes)} memory</p>
        {:else}
          <p class="muted">No local process — observed as a connection endpoint.</p>
        {/if}
        <p>{peerLinks.length} observed {peerLinks.length === 1 ? "link" : "links"} · No listening ports reported</p>
      </section>
      {#if peerNeighbors.length}
        <section class="peer-sec" aria-labelledby="peer-neighbors-head">
          <h4 id="peer-neighbors-head">Connected services <span class="n">{peerNeighbors.length}</span></h4>
          <ul class="neighbors">
            {#each peerNeighbors as { node, direction } (node.id)}
              <li><button type="button" class="neighbor" onclick={() => selectNeighbor(node)} title="Select {node.label} on the graph">
                <span class="nb-main"><span class="nb-label">{node.label}</span><span class="nb-sub">{direction}{node.ports.length ? ` · :${node.ports.map((p) => p.port).join(", :")}` : node.kind === "external" ? " · remote hosts" : ""}</span></span>
                <Icon name="arrow" size={12} />
              </button></li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if peerRemotes.length}
        <section class="peer-sec" aria-labelledby="peer-remote-head">
          <h4 id="peer-remote-head">Remote endpoints <span class="n">{peerRemotes.length}</span></h4>
          <ul class="remotes">{#each peerRemotes as remote}<li class="mono remote">{remote}</li>{/each}</ul>
        </section>
      {/if}
      <small>Connections show observed sockets, not messages or tool calls.</small>
    </aside>
  {/if}
</section>

<style>
  .graph { display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); position: relative; height: 100%; min-height: 0; min-width: 0; background: var(--bg); container-type: inline-size; outline: none; }
  .graph.has-peer { grid-template-columns: minmax(0, 1fr) minmax(300px, 340px); }
  .graph:focus-visible { box-shadow: inset 0 0 0 2px var(--ring); }
  h3, h4, p { margin: 0; }
  .graph-body { position: relative; min-height: 0; min-width: 0; }
  .graph :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-node-border-radius: 14px; --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .graph :global(.svelte-flow__node-service), .graph :global(.svelte-flow__node-cluster) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; }
  .graph :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  .graph :global(.svelte-flow__controls-button) { width: 34px; height: 34px; }
  .graph :global(.svelte-flow__controls-button.readable-zoom) { width: 54px; font: inherit; font-size: var(--fs-caption); font-variant-numeric: tabular-nums; border-right: 1px solid var(--border); }
  .graph :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); opacity: .85; }
  .graph :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }

  /* Floating chrome: frosted, hairline and compact so the canvas keeps the room. */
  .chrome-host { position: absolute; top: 10px; left: 10px; right: 10px; z-index: 6; pointer-events: none; }
  .chrome { position: relative; display: flex; flex-direction: column; align-items: flex-start; gap: 5px; max-width: min(100%, 620px); }
  .chrome > * { pointer-events: auto; }
  .bar, .stats, .legend, .search-context, .connection-note, .cluster-focus { background: var(--overlay); border: 1px solid var(--border); box-shadow: var(--shadow-sm); -webkit-backdrop-filter: blur(8px); backdrop-filter: blur(8px); }
  .bar { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; max-width: 100%; box-sizing: border-box; padding: 3px; border-radius: var(--r-full); }
  .stats { display: flex; flex-wrap: wrap; gap: 2px 12px; max-width: 100%; box-sizing: border-box; padding: 4px 12px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .stats b { color: var(--text); font-weight: var(--fw-semibold); }
  .updating { color: var(--accent); font-weight: var(--fw-medium); }
  .legend { display: flex; flex-wrap: wrap; gap: 2px 12px; max-width: 100%; box-sizing: border-box; padding: 4px 12px; border-radius: var(--r-lg); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lg { display: inline-flex; align-items: center; gap: 5px; }
  .lg i { width: 10px; height: 10px; border-radius: 3px; border: 1.5px solid var(--border-strong); background: var(--surface); }
  .lg-service i { border-left: 3px solid var(--tone-green); }
  .lg-client i { border-style: dashed; }
  .lg-external i { border-style: dashed; background: var(--surface-2); }
  .options-wrap { display: inline-flex; }
  .options-trigger { display: inline-flex; align-items: center; gap: 5px; height: 28px; padding: 0 10px; border-radius: var(--r-full); border: 0; background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .options-trigger:hover, .options-trigger[aria-expanded="true"] { background: var(--surface-2); color: var(--text); }
  .options-trigger:focus-visible, .cta:focus-visible, .search-context button:focus-visible, .cluster-focus button:focus-visible, .neighbor:focus-visible, .peer-agent:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .options { position: absolute; top: calc(100% + 6px); left: 0; z-index: 8; width: min(320px, calc(100cqw - 20px)); box-sizing: border-box; border: 1px solid var(--border-strong); border-radius: var(--r-lg); background: var(--surface); box-shadow: var(--shadow-md); padding: 12px; color: var(--text); }
  .options-heading { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 4px; font-size: var(--fs-body); }
  .option { display: flex; gap: 16px; justify-content: space-between; align-items: center; padding: 12px 0; border-top: 1px solid var(--border); }
  .option strong { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); }
  .option p { margin-top: 3px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .search-context, .cluster-focus, .connection-note { display: flex; align-items: center; gap: 6px; max-width: 100%; box-sizing: border-box; padding: 3px 4px 3px 10px; border-radius: var(--r-full); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .search-context, .cluster-focus { border-color: color-mix(in srgb, var(--accent) 35%, var(--border)); }
  .search-context strong, .cluster-focus strong { color: var(--text); }
  .search-context button, .cluster-focus button { flex: none; min-height: 22px; padding: 0 8px; border: 0; border-radius: var(--r-full); background: var(--surface); color: var(--accent); font: inherit; font-weight: var(--fw-medium); cursor: pointer; }
  .connection-note { padding-right: 10px; border-radius: var(--r-lg); color: var(--muted); }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 10px; color: var(--muted); text-align: center; padding: 96px 20px 20px; box-sizing: border-box; }
  .empty .art { width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--surface-3); color: var(--muted); }
  .empty h3 { color: var(--text); font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); }
  .empty p { font-size: var(--fs-body); line-height: var(--lh-body); max-width: 360px; text-wrap: pretty; }
  .cta { height: var(--h-sm); padding: 0 12px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }

  /* Peer details dock on the right like the port details pane and the agent inspector. */
  .peer-inspector { min-height: 0; overflow-y: auto; overscroll-behavior: contain; box-sizing: border-box; padding: 14px 16px 18px; border-left: 1px solid var(--border); background: var(--surface); color: var(--text-2); box-shadow: var(--shadow-sm); display: flex; flex-direction: column; gap: 12px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); z-index: 7; }
  .peer-inspector header { display: grid; grid-template-columns: auto 1fr auto; gap: 10px; align-items: center; }
  .peer-tile { width: 36px; height: 36px; display: grid; place-items: center; border-radius: 10px; border: 1px solid var(--border); background: var(--surface-2); color: var(--accent); }
  .peer-who { min-width: 0; display: grid; }
  .peer-inspector h3 { color: var(--text); font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); overflow-wrap: anywhere; }
  .eyebrow { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .peer-agent { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 10px; border: 1px solid color-mix(in srgb, var(--accent) 35%, var(--border)); border-radius: var(--r-md); background: var(--accent-softer); color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); text-align: left; cursor: pointer; }
  .peer-agent b { color: var(--text); }
  .peer-agent .go { margin-left: auto; color: var(--accent); font-weight: var(--fw-semibold); white-space: nowrap; }
  .peer-agent:hover { background: var(--accent-soft); }
  .peer-sec { display: grid; gap: 6px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .peer-sec h4 { display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .peer-sec .n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .muted { color: var(--muted); }
  .neighbors, .remotes { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .neighbor { display: flex; align-items: center; gap: 8px; padding: 5px 8px; margin: 0 -8px; width: calc(100% + 16px); border: 0; border-radius: var(--r-sm); background: transparent; color: var(--muted); font: inherit; text-align: left; cursor: pointer; }
  .neighbor:hover { background: var(--surface-2); color: var(--text); }
  .nb-main { flex: 1; min-width: 0; display: grid; }
  .nb-label { color: var(--text); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nb-sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .peer-inspector small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .peer-inspector .remote { overflow-wrap: anywhere; font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  @container (max-width: 900px) {
    .graph.has-peer { grid-template-columns: minmax(0, 1fr); }
    .peer-inspector { position: absolute; top: 8px; right: 8px; bottom: 8px; width: min(340px, calc(100% - 16px)); border: 1px solid var(--border-strong); border-radius: var(--r-lg); box-shadow: var(--shadow-md); }
    .graph.has-peer :global(.svelte-flow__minimap) { display: none; }
  }
  @container (max-width: 640px) {
    .peer-inspector { top: auto; left: 8px; width: auto; max-height: min(58%, 440px); }
    .bar { border-radius: var(--r-lg); }
    .opt-label, .edge-key, .stat-groups { display: none; }
    .graph :global(.svelte-flow__minimap) { display: none; }
  }
  .reduced :global(*) { animation: none !important; }
</style>
