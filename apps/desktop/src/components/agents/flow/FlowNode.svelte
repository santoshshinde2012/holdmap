<script lang="ts">
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import Icon from "../../Icon.svelte";
  import { STATUS_LABEL } from "../../../lib/agent-flow/model";
  import type { FlowNode } from "../../../lib/agent-flow/layout";
  let { data }: NodeProps<FlowNode> = $props();
  const n = $derived(data.node);
  const icons: Record<string, string> = { agent: "bot", tool: "plug", process: "terminal", listener: "server", remote: "globe", unknown: "info", model: "sparkles", handoff: "arrow", step: "activity", skill: "sparkles" };
</script>

<div class="flow-node" class:chosen={data.chosen} class:inferred={!n.span && n.evidence !== "observed"} class:failed={n.status === "error"}>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <span class="icon"><Icon name={icons[n.kind] ?? "info"} size={17} /></span>
  <div class="copy"><strong title={n.label}>{n.label}</strong><span title={n.sub}>{n.sub}</span>
    <small>{n.span ? `${n.kind} · reported` : `${n.kind} · ${n.evidence}`}</small>
  </div>
  {#if n.status === "error"}<span class="error" title={STATUS_LABEL.error}><Icon name="alert" size={13} /></span>{/if}
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>

<style>
  .flow-node { width: 244px; height: 78px; display: flex; align-items: center; gap: 10px; box-sizing: border-box; padding: 10px 12px; border-radius: 12px; border: 1px solid var(--border-strong); background: var(--surface); box-shadow: var(--shadow-sm); color: var(--text); cursor: pointer; }
  .chosen { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft), var(--shadow-sm); }
  .inferred { border-style: dashed; }
  .failed { border-color: var(--danger); }
  .icon { display: grid; place-items: center; flex: none; width: 32px; height: 32px; border-radius: 10px; color: var(--accent); background: var(--accent-soft); }
  .copy { display: grid; gap: 3px; min-width: 0; flex: 1; }
  strong { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  strong, .copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .copy span, small { font-size: var(--fs-caption); color: var(--muted); }
  small { text-transform: capitalize; }
  .error { color: var(--danger); }
  .flow-node :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
</style>
