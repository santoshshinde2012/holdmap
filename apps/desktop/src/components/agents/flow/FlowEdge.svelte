<script lang="ts">
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps } from "@xyflow/svelte";
  import type { FlowEdge } from "../../../lib/agent-flow/layout";
  let { sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, id }: EdgeProps<FlowEdge> = $props();
  const path = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
  const e = $derived(data?.edge);
</script>

<BaseEdge {id} path={path[0]} interactionWidth={20} style={`stroke: ${data?.chosen ? "var(--accent)" : "var(--border-strong)"}; stroke-width: ${data?.chosen ? 2.5 : 1.5}; ${e?.evidence === "inferred" || e?.kind === "span_parent" ? "stroke-dasharray: 5 4;" : ""}`} />
{#if data?.labelled !== false}<EdgeLabel x={path[1]} y={path[2]}><span class="relationship-label" class:chosen={data?.chosen}>{e?.label}</span></EdgeLabel>{/if}

<style>
  .relationship-label { padding: 3px 7px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text-2); font-size: var(--fs-caption); white-space: nowrap; }
  .chosen { color: var(--accent); border-color: var(--accent); }
</style>
