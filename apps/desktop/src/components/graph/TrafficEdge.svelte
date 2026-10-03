<script lang="ts">
  // Directed dependency edge with an optional "live traffic" animation: dots travel from the
  // client to the service it uses, more dots for more open connections.
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps, type Edge } from "@xyflow/svelte";
  import { edgeLabel, trafficDots, type FlowEdgeData } from "../../lib/graph";

  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, markerEnd }: EdgeProps<Edge<FlowEdgeData>> = $props();
  const geo = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
  const e = $derived(data!.edge);
  const dots = $derived(trafficDots(e.connections));
  const outbound = $derived(e.kind === "outbound");
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
<EdgeLabel x={geo[1]} y={geo[2]} class="edge-label {data?.dim ? 'dim' : ''} {data?.hl ? 'hl' : ''}">
  <span class="mono">{edgeLabel(e)}</span>
</EdgeLabel>

<style>
  :global(.svelte-flow__edge-path.traffic) { stroke: var(--edge, #98a2b3); stroke-width: 1.6; transition: opacity 160ms, stroke 160ms; }
  :global(.svelte-flow__edge-path.traffic.outbound) { stroke-dasharray: 5 5; }
  :global(.svelte-flow__edge-path.traffic.hl) { stroke: var(--accent); stroke-width: 2.4; }
  :global(.svelte-flow__edge-path.traffic.dim) { opacity: 0.18; }
  .dot { fill: var(--accent); filter: drop-shadow(0 0 3px var(--accent)); }
  .dot.outbound { fill: var(--tone-violet); }
  :global(.edge-label) {
    font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 1px 6px; border-radius: 6px; background: var(--surface); color: var(--muted);
    border: 1px solid var(--border); pointer-events: none; transition: opacity 160ms;
    max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-variant-numeric: tabular-nums;
  }
  :global(.edge-label.hl) { color: var(--accent); border-color: var(--accent); }
  :global(.edge-label.dim) { opacity: 0.2; }
</style>
