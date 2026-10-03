<script lang="ts">
  // Directed dependency edge with an optional "live traffic" animation: dots travel from the
  // client to the service it uses, more dots for more open connections.
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps, type Edge } from "@xyflow/svelte";
  import { edgeLabel, fitEdgeLabel, trafficDots, type FlowEdgeData } from "../../lib/graph";

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, markerEnd }: EdgeProps<Edge<FlowEdgeData>> = $props();
  const geo = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
  const e = $derived(data!.edge);
  const dots = $derived(trafficDots(e.connections));
  const outbound = $derived(e.kind === "outbound");
  const full = $derived(edgeLabel(e));
  const fit = $derived(fitEdgeLabel(full, sourceX, sourceY, targetX, targetY));
</script>

<BaseEdge
  {id}
  path={geo[0]}
  {markerEnd}
  class="traffic {outbound ? 'outbound' : ''} {data?.dim ? 'dim' : ''} {data?.hl ? 'hl' : ''}"
/>
{#if data?.animate && !data?.dim}
  {#each Array(dots.count) as _, i}
    <circle r={data?.hl ? 3.6 : 2.8} class="dot" class:outbound>
      <animateMotion dur="{dots.duration}s" repeatCount="indefinite" begin="{(i * dots.duration) / dots.count}s" path={geo[0]} />
    </circle>
  {/each}
{/if}
<EdgeLabel x={geo[1]} y={geo[2]} class="edge-label {fit.clipped ? 'clipped' : ''} {data?.dim ? 'dim' : ''} {data?.hl ? 'hl' : ''}">
  <span class="mono" title={fit.clipped ? full : undefined}>{fit.text}</span>
</EdgeLabel>

<style>
  :global(.svelte-flow__edge-path.traffic) { stroke: var(--edge, #98a2b3); stroke-width: 1.6; transition: opacity var(--dur-2), stroke var(--dur-2); }
  :global(.svelte-flow__edge-path.traffic.outbound) { stroke-dasharray: 5 5; }
  :global(.svelte-flow__edge-path.traffic.hl) { stroke: var(--accent); stroke-width: 2.4; }
  :global(.svelte-flow__edge-path.traffic.dim) { opacity: 0.25; }
  .dot { fill: var(--accent); filter: drop-shadow(0 0 3px var(--accent)); }
  .dot.outbound { fill: var(--tone-violet); }
  :global(.edge-label) {
    font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 1px 6px; border-radius: 6px; background: var(--surface); color: var(--muted);
    border: 1px solid var(--border); pointer-events: none; transition: opacity var(--dur-2);
    max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-variant-numeric: tabular-nums;
  }
  /* A shortened label shows the full text on hover. */
  :global(.edge-label.clipped) { pointer-events: auto; cursor: default; }
  :global(.edge-label.hl) { color: var(--accent); border-color: var(--accent); }
  :global(.edge-label.dim) { opacity: 0.35; }
</style>
