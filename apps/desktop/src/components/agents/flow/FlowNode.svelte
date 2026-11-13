<script lang="ts">
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import Icon from "../../Icon.svelte";
  import { STATUS_LABEL } from "../../../lib/agent-flow/model";
  import type { FlowNode } from "../../../lib/agent-flow/layout";
  let { data }: NodeProps<FlowNode> = $props();
  const n = $derived(data.node);
  const icons: Record<string, string> = { agent: "bot", tool: "plug", process: "terminal", listener: "server", remote: "globe", unknown: "info", model: "sparkles", handoff: "arrow", step: "activity", skill: "sparkles" };
</script>

<div class="flow-node k-{n.kind}" class:chosen={data.chosen} class:dim={data.dim} class:inferred={!n.span && n.evidence !== "observed"} class:failed={n.status === "error"}>
  <Handle type="target" position={Position.Left} isConnectable={false} aria-hidden="true" />
  <span class="icon"><Icon name={icons[n.kind] ?? "info"} size={17} /></span>
  <div class="copy"><strong title={n.label}>{n.label}</strong><span title={n.sub}>{n.sub}</span>
    <small>{n.span ? `${n.kind} · reported` : `${n.kind} · ${n.evidence}`}</small>
  </div>
  {#if n.status === "error"}<span class="error" title={STATUS_LABEL.error}><Icon name="alert" size={13} /></span>{/if}
  <Handle type="source" position={Position.Right} isConnectable={false} aria-hidden="true" />
</div>

<style>
  /* Each kind has its own hue on the icon and a left rail, so a busy map scans by type. */
  .flow-node { --k: var(--muted); width: 244px; height: 78px; display: flex; align-items: center; gap: 10px; box-sizing: border-box; padding: 10px 12px 10px 13px; border-radius: 12px; border: 1px solid var(--border-strong); border-left: 3px solid var(--k); background: var(--surface); box-shadow: var(--shadow-sm); color: var(--text); cursor: pointer; transition: opacity var(--dur-1) var(--ease), border-color var(--dur-1) var(--ease); }
  .k-agent { --k: var(--agent-1); } .k-tool { --k: var(--tone-violet); } .k-listener { --k: var(--tone-green); } .k-remote { --k: var(--tone-blue); } .k-skill, .k-model { --k: var(--tone-amber); }
  .chosen { border-color: var(--k); box-shadow: 0 0 0 3px color-mix(in srgb, var(--k) 22%, transparent), var(--shadow-sm); }
  .dim { opacity: 0.38; }
  .inferred { border-style: dashed; }
  .failed { border-color: var(--danger); border-left-color: var(--danger); }
  .icon { display: grid; place-items: center; flex: none; width: 32px; height: 32px; border-radius: 10px; color: var(--k); background: color-mix(in srgb, var(--k) 13%, var(--surface)); }
  .copy { display: grid; gap: 3px; min-width: 0; flex: 1; }
  strong { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  strong, .copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .copy span, small { font-size: var(--fs-caption); color: var(--muted); }
  small { text-transform: capitalize; }
  .error { color: var(--danger); }
  .flow-node :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
</style>
