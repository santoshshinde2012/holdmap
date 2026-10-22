<script lang="ts">
  // One card in an analyzer graph: stage, title, real numbers and a status edge. The whole card
  // is a button, so it is reachable with Tab and opens its details with Enter or Space.
  import { Handle, Position, type Node, type NodeProps } from "@xyflow/svelte";
  import Icon from "../../Icon.svelte";
  import { SEVERITY_ORDER, total } from "../../../lib/analyzers";
  import { STAGE_LABEL, type GraphNode } from "../../../lib/analyzer-graph";

  let { data }: NodeProps<Node<{ node: GraphNode; selected: boolean; onactivate: () => void; first: boolean; last: boolean }>> = $props();
  const node = $derived(data.node);
  const ICON: Record<GraphNode["kind"], string> = { sources: "layers", projection: "filter", sandbox: "lock", analyzer: "shield", parser: "code", findings: "alert", surface: "panel", agent: "bot" };
  const stage = $derived(node.kind === "agent" ? "Agent" : node.kind === "analyzer" ? "Analyzer" : STAGE_LABEL[node.kind]);
  const sum = $derived(node.counts ? total(node.counts) : 0);
</script>

<div class="wrap" style="--w: {node.width}px; --h: {node.height}px">
  {#if !data.first}<Handle type="target" position={Position.Left} isConnectable={false} />{/if}
  <button type="button" class="card nodrag nopan st-{node.status} k-{node.kind}" class:selected={data.selected}
    data-graph-node={node.id} aria-pressed={data.selected}
    aria-label={`${stage}: ${node.title}. ${node.subtitle}. ${node.badges.join(", ")}`}
    onclick={(e) => { e.stopPropagation(); data.onactivate(); }}>
    <span class="tile" aria-hidden="true"><Icon name={ICON[node.kind]} size={15} /></span>
    <span class="copy">
      <span class="stage">{stage}</span>
      <strong title={node.title}>{node.title}</strong>
      <span class="sub" title={node.subtitle}>{node.subtitle}</span>
      {#if sum && node.counts}
        <span class="bar" aria-hidden="true">{#each SEVERITY_ORDER.filter((s) => node.counts?.[s]) as s}<i class="sev-{s}" style="flex: {node.counts[s]}"></i>{/each}</span>
      {:else if node.badges.length}
        <span class="badges">{#each node.badges.slice(0, 2) as b}<em title={b}>{b}</em>{/each}</span>
      {/if}
    </span>
  </button>
  {#if !data.last}<Handle type="source" position={Position.Right} isConnectable={false} />{/if}
</div>

<style>
  .wrap { position: relative; width: var(--w); height: var(--h); }
  .card { width: 100%; height: 100%; box-sizing: border-box; display: flex; gap: 10px; align-items: flex-start; padding: 12px; border: 1px solid var(--border-strong); border-left: 3px solid var(--tone, var(--border-strong)); border-radius: 14px; background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); font: inherit; text-align: left; cursor: pointer; transition: border-color var(--dur-2) var(--ease), box-shadow var(--dur-2) var(--ease); }
  .card:hover { box-shadow: var(--shadow-md); }
  .card:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .card.selected { box-shadow: 0 0 0 2px var(--tone, var(--accent)), var(--shadow-md); }
  .st-ok { --tone: var(--ok); }
  .st-warn { --tone: var(--warn); }
  .st-error { --tone: var(--danger); }
  .st-idle { --tone: var(--border-strong); }
  .tile { width: 28px; height: 28px; flex: none; display: grid; place-items: center; border-radius: 8px; color: var(--accent-text, var(--accent)); background: var(--accent-soft); }
  .k-sandbox .tile { color: var(--tone-violet, var(--accent)); background: color-mix(in srgb, var(--tone-violet, var(--accent)) 12%, var(--surface)); }
  .k-findings .tile, .k-agent .tile { color: var(--text-2); background: var(--surface-2); }
  .copy { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .stage { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); }
  strong { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); }
  strong, .sub, em { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .sub { color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .badges { display: flex; gap: 4px; min-width: 0; margin-top: 2px; }
  em { font-style: normal; font-size: var(--fs-label); padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r-full); color: var(--muted); max-width: 100%; }
  .bar { display: flex; height: 6px; margin-top: 4px; border-radius: 3px; overflow: hidden; background: var(--surface-2); }
  .bar i { display: block; }
  .sev-critical, .sev-high { background: var(--danger); }
  .sev-medium { background: var(--warn); }
  .sev-low { background: var(--accent); }
  .sev-info { background: var(--border-strong); }
  .wrap :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .card { transition: none; } }
</style>
