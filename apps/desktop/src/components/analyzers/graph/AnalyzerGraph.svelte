<script lang="ts">
  // Draws an analyzer graph (pipeline or overview) with Svelte Flow, plus a details panel for
  // the selected card. Cards are buttons: Tab or the arrow keys move between them, Enter opens
  // details. Narrow containers get a stacked list in reading order instead of the canvas.
  import { SvelteFlow, Background, Controls, MarkerType, type Node, type Edge, type NodeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { onMount } from "svelte";
  import AnalyzerGraphNode from "./AnalyzerGraphNode.svelte";
  import Icon from "../../Icon.svelte";
  import { readingOrder, STAGE_LABEL, type Graph, type GraphNode } from "../../../lib/analyzer-graph";

  let { graph, label, selected = $bindable(null), height = 380, onopen }: {
    graph: Graph;
    /** Accessible name of the graph. */
    label: string;
    selected?: string | null;
    height?: number;
    /** Secondary action for a card (e.g. open an analyzer's pipeline from the overview). */
    onopen?: (node: GraphNode) => { label: string; run: () => void } | null;
  } = $props();

  let width = $state(0);
  let dark = $state(false);
  let root: HTMLDivElement | undefined = $state();
  const narrow = $derived(width > 0 && width < 640);
  const order = $derived(readingOrder(graph));
  const current = $derived(graph.nodes.find((n) => n.id === selected) ?? null);
  const action = $derived(current && onopen ? onopen(current) : null);
  const targets = $derived(new Set(graph.edges.map((e) => e.to)));
  const sources = $derived(new Set(graph.edges.map((e) => e.from)));

  onMount(() => {
    const el = document.documentElement;
    const read = () => (dark = el.dataset.theme === "dark");
    read();
    const mo = new MutationObserver(read);
    mo.observe(el, { attributes: true, attributeFilter: ["data-theme"] });
    // Width is read on the next frame: switching layouts inside the observer callback would
    // trigger a "ResizeObserver loop" error.
    let frame = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => (width = root?.clientWidth ?? 0));
    });
    if (root) { width = root.clientWidth; ro.observe(root); }
    return () => { mo.disconnect(); ro.disconnect(); cancelAnimationFrame(frame); };
  });

  const nodeTypes = { stage: AnalyzerGraphNode } as unknown as NodeTypes;
  const tone = (s: string | null) => (s === "critical" || s === "high" ? "var(--danger)" : s === "medium" ? "var(--warn)" : "var(--border-strong)");
  const flowNodes = $derived<Node[]>(graph.nodes.map((n) => ({
    id: n.id, type: "stage", position: n.position, width: n.width, height: n.height,
    draggable: false, connectable: false, selectable: false, focusable: false,
    data: { node: n, selected: n.id === selected, first: !targets.has(n.id), last: !sources.has(n.id), onactivate: () => (selected = selected === n.id ? null : n.id) },
  })));
  const flowEdges = $derived<Edge[]>(graph.edges.map((e) => ({
    id: e.id, source: e.from, target: e.to, label: e.label ?? undefined, animated: false, selectable: false, focusable: false,
    style: `stroke: ${e.active ? tone(e.severity) : "var(--border)"}; stroke-width: ${e.active ? 1.6 : 1}; ${e.active ? "" : "stroke-dasharray: 4 4;"}`,
    markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14, color: e.active ? tone(e.severity) : "var(--border)" },
    labelStyle: "font-size: 11px;",
  })));
  let nodes = $state.raw<Node[]>([]), edges = $state.raw<Edge[]>([]);
  $effect(() => { nodes = flowNodes; edges = flowEdges; });

  function onkeydown(e: KeyboardEvent) {
    const id = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-graph-node]")?.dataset.graphNode;
    if (!id) return;
    if (e.key === "Escape" && selected) { selected = null; e.preventDefault(); return; }
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    const i = order.findIndex((n) => n.id === id);
    const next = order[(i + step + order.length) % order.length];
    root?.querySelector<HTMLElement>(`[data-graph-node="${CSS.escape(next.id)}"]`)?.focus();
    e.preventDefault();
  }
  // Arrow keys from a focused card (delegated: the cards are the interactive elements).
  $effect(() => {
    const el = root;
    if (!el) return;
    el.addEventListener("keydown", onkeydown);
    return () => el.removeEventListener("keydown", onkeydown);
  });
  const stageOf = (n: GraphNode) => (n.kind === "agent" ? "Agent" : n.kind === "analyzer" ? "Analyzer" : STAGE_LABEL[n.kind]);
</script>

<div class="ag" bind:this={root} role="group" aria-label={label}>
  {#if narrow}
    <ol class="stack" aria-label={`${label} (list)`}>
      {#each order as n, i (n.id)}
        <li>
          <button type="button" class="row st-{n.status}" data-graph-node={n.id} aria-expanded={selected === n.id} onclick={() => (selected = selected === n.id ? null : n.id)}>
            <span class="stage">{stageOf(n)}</span>
            <strong>{n.title}</strong>
            <span class="sub">{n.subtitle}</span>
          </button>
          {#if selected === n.id}
            <dl class="lines">{#each n.lines as l}<div><dt>{l.label}</dt><dd>{l.value}</dd></div>{/each}</dl>
            {#if action}<button type="button" class="link" onclick={action.run}>{action.label}</button>{/if}
          {/if}
          {#if i < order.length - 1 && order[i + 1].position.x !== n.position.x}<span class="arrow" aria-hidden="true">↓</span>{/if}
        </li>
      {/each}
    </ol>
  {:else}
    <div class="canvas" style="height: {height}px">
      <SvelteFlow bind:nodes bind:edges {nodeTypes} colorMode={dark ? "dark" : "light"}
        fitView fitViewOptions={{ padding: 0.08, maxZoom: 1 }} minZoom={0.2} maxZoom={1.6}
        nodesDraggable={false} nodesConnectable={false} nodesFocusable={false} edgesFocusable={false} elementsSelectable={false} deleteKey={null}
        proOptions={{ hideAttribution: true }}
        ariaLabelConfig={{ "controls.fitView.ariaLabel": "Fit the graph in view", "controls.zoomIn.ariaLabel": "Zoom in", "controls.zoomOut.ariaLabel": "Zoom out" }}>
        <Background gap={22} size={1.2} />
        <Controls showLock={false} position="bottom-left" />
      </SvelteFlow>
      <p class="hint"><Icon name="info" size={12} /><span>Tab or arrow keys move between cards · Enter shows details</span></p>
    </div>
    {#if current}
      <aside class="details" aria-label={`Details: ${current.title}`}>
        <header><span class="stage">{stageOf(current)}</span><strong>{current.title}</strong><button type="button" class="x" aria-label="Close details" onclick={() => (selected = null)}><Icon name="x" size={12} /></button></header>
        <dl class="lines">{#each current.lines as l}<div><dt>{l.label}</dt><dd>{l.value}</dd></div>{/each}</dl>
        {#if action}<button type="button" class="link" onclick={action.run}>{action.label}</button>{/if}
      </aside>
    {/if}
  {/if}
</div>

<style>
  .ag { display: grid; gap: var(--sp-2); min-width: 0; }
  .canvas { position: relative; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; background: var(--bg); }
  .canvas :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-edge-label-background-color: var(--surface); --xy-edge-label-color: var(--text-2); }
  .canvas :global(.svelte-flow__node-stage) { padding: 0; border: 0; background: transparent; box-shadow: none; }
  .canvas :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .canvas :global(.svelte-flow__controls) { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; box-shadow: var(--shadow-sm); }
  .hint { position: absolute; top: 8px; left: 8px; margin: 0; display: flex; gap: 6px; align-items: center; padding: 4px 8px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); color: var(--muted); font-size: var(--fs-caption); pointer-events: none; }
  .details { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: 10px var(--sp-3); }
  .details header { display: flex; align-items: baseline; gap: var(--sp-2); margin-bottom: 6px; }
  .x { margin-left: auto; border: 0; background: none; color: var(--muted); cursor: pointer; padding: 2px; }
  .x:focus-visible, .link:focus-visible, .row:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .stage { color: var(--muted); font-size: var(--fs-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-medium); }
  .lines { margin: 0; display: grid; gap: 2px; }
  .lines > div { display: grid; grid-template-columns: 150px minmax(0, 1fr); gap: var(--sp-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  dt { color: var(--muted); overflow-wrap: anywhere; }
  dd { margin: 0; color: var(--text-2); overflow-wrap: anywhere; font-family: inherit; }
  .link { margin-top: 6px; border: 0; background: none; padding: 0; color: var(--accent-text, var(--accent)); font: inherit; font-size: var(--fs-body-sm); cursor: pointer; text-decoration: underline; }
  .stack { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .stack li { display: grid; gap: 4px; }
  .row { display: grid; gap: 1px; width: 100%; padding: 8px 10px; border: 1px solid var(--border); border-left: 3px solid var(--tone, var(--border-strong)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .row strong { font-size: var(--fs-body-sm); }
  .row .sub { color: var(--text-2); font-size: var(--fs-caption); }
  .st-ok { --tone: var(--ok); } .st-warn { --tone: var(--warn); } .st-error { --tone: var(--danger); }
  .stack .lines { padding: 4px 10px; }
  .stack .lines > div { grid-template-columns: minmax(0, 1fr); gap: 0; }
  .arrow { justify-self: center; color: var(--muted); font-size: var(--fs-caption); }
</style>
