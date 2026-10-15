<script lang="ts">
  import { BaseEdge, EdgeLabel, getBezierPath, type Edge, type EdgeProps } from "@xyflow/svelte";
  import type { CapabilityGraphEdge } from "../../../lib/agent-capability-map";
  let { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps<Edge<{
    edge: CapabilityGraphEdge; color: string; dim: boolean; highlighted: boolean;
  }>> = $props();
  const geometry = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
</script>

<BaseEdge {id} path={geometry[0]} interactionWidth={0} style="stroke: {data!.color}; stroke-width: {data!.highlighted ? 2.3 : 1.5}; opacity: {data!.dim ? .12 : data!.highlighted ? .9 : .52}; transition: opacity var(--dur-2) var(--ease);" />
{#if data!.highlighted && !data!.dim}<EdgeLabel x={geometry[1]} y={geometry[2]} class="capability-edge-label"><span>{data!.edge.label}</span></EdgeLabel>{/if}

<style>
  :global(.capability-edge-label) { pointer-events: none; }
  :global(.capability-edge-label span) { display: inline-block; padding: 2px 6px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); box-shadow: var(--shadow-sm); }
</style>
