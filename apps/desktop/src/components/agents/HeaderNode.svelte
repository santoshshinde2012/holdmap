<script lang="ts">
  // Column title in the footprint graph ("Folders", "Agents"…); not interactive.
  import type { NodeProps, Node } from "@xyflow/svelte";
  import type { FootFlowData } from "../../lib/agents";
  let { data }: NodeProps<Node<FootFlowData>> = $props();
</script>

<div class="hd">{data.node.label}</div>

<style>
  .hd { width: 216px; text-align: center; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); pointer-events: none; }
</style>
