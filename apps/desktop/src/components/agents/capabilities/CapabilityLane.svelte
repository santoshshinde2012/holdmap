<script lang="ts">
  import type { Node, NodeProps } from "@xyflow/svelte";
  import Icon from "../../Icon.svelte";
  import type { CapabilityGraphLane } from "../../../lib/agent-capability-map";
  let { data }: NodeProps<Node<{ lane: CapabilityGraphLane }>> = $props();
  const lane = $derived(data.lane);
</script>

<div class="lane k-{lane.id}" style="width: {lane.width}px; height: {lane.height}px" aria-hidden="true">
  <header><span><Icon name={lane.id === "agents" ? "bot" : lane.id === "mcp" ? "plug" : lane.id === "skill" ? "sparkles" : "cpu"} size={13} />{lane.label}</span><b>{lane.count}</b></header>
  <p>{lane.sub}</p>
</div>

<style>
  .lane { box-sizing: border-box; border: 1px solid var(--border); border-radius: 20px; background: color-mix(in srgb, var(--surface) 68%, transparent); color: var(--text-2); padding: 15px 20px; pointer-events: none; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 9px; font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); font-weight: var(--fw-semibold); }
  header span { display: flex; align-items: center; gap: 6px; } b { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  p { margin: 6px 0 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
  .k-agents { background: transparent; border-color: transparent; }
  .k-skill header { color: var(--tone-violet); }
  .k-running { border-style: dashed; background: color-mix(in srgb, var(--surface-2) 55%, transparent); }
</style>
