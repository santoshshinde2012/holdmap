<script lang="ts">
  // Edge between an agent and its footprint, in the agent's colour. The path is precomputed by
  // the layout (lib/agents.ts), so it ignores handle coordinates. Dashed: recent, inferred or remote.
  import { BaseEdge, EdgeLabel, type EdgeProps, type Edge } from "@xyflow/svelte";
  import type { FootFlowEdgeData } from "../../lib/agents";

  let { id, data }: EdgeProps<Edge<FootFlowEdgeData>> = $props();
  const e = $derived(data!.edge);
  const dashed = $derived(e.kind === "recent" || e.kind === "remote" || e.inferred);
  const width = $derived(e.kind === "parent" ? 2.2 : e.connections > 2 ? 2 : 1.5);
</script>

<BaseEdge
  {id}
  path={data!.geometry.path}
  interactionWidth={0}
  style="stroke: {data!.color}; stroke-width: {data!.hl ? width + 0.8 : width}; stroke-dasharray: {dashed ? '5 4' : 'none'}; opacity: {data!.dim ? 0.15 : e.kind === 'recent' ? 0.55 : 0.85}; transition: opacity var(--dur-2) var(--ease);"
/>
{#if e.label && !data!.dim}
  <EdgeLabel x={data!.geometry.labelX} y={data!.geometry.labelY} class="foot-label {data!.hl ? 'hl' : ''}">
    <span style="--agent: {data!.color}">{e.label}</span>
  </EdgeLabel>
{/if}

<style>
  :global(.foot-label) { pointer-events: none; }
  :global(.foot-label span) {
    display: inline-block; padding: 0 6px; border-radius: 6px; background: var(--surface); border: 1px solid var(--border);
    color: var(--text-2); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums;
  }
  :global(.foot-label.hl span) { border-color: var(--agent); color: var(--text); }
</style>
