<script lang="ts">
  import { Handle, Position, type Node, type NodeProps } from "@xyflow/svelte";
  import Icon from "../../Icon.svelte";
  import { EVIDENCE_LABEL, monogram } from "../../../lib/agents";
  import { enabledLabel, pathName, scopeLabel } from "../../../lib/agent-capabilities";
  import type { CapabilityGraphNode } from "../../../lib/agent-capability-map";

  let { data }: NodeProps<Node<{
    node: CapabilityGraphNode; color: string; selected: boolean; dim: boolean; highlighted: boolean;
    label: string; onactivate: () => void; onfocus: (event: FocusEvent) => void;
  }>> = $props();
  const node = $derived(data.node);
  const icon = $derived(node.kind === "skill" ? "sparkles" : node.kind === "running" ? "cpu" : "plug");
  const subtitle = $derived(node.item ? `${node.item.owners.map((owner) => owner.name).filter((name, index, names) => names.indexOf(name) === index).join(", ")} · ${node.item.projectPath ? pathName(node.item.projectPath) : `${scopeLabel[node.item.scope]} scope`}` : node.runtime ? "Observed MCP candidate" : node.sub);
  const status = $derived(node.item ? enabledLabel(node.item) : node.runtime ? `PID ${node.runtime.tool.pid} · ${EVIDENCE_LABEL[node.runtime.tool.evidence]}` : node.agent?.capabilities?.inspected ? `${node.agent.capabilities.mcp_servers.length} MCP · ${node.agent.capabilities.skills.length} skills reported` : "Inventory not inspected");
</script>

<div class="node-wrap k-{node.kind}" style="--node-w: {node.width}px; --node-h: {node.height}px; --agent: {data.color}">
  {#if node.kind === "mcp" || node.kind === "skill"}<Handle type="target" position={node.kind === "mcp" ? Position.Right : Position.Left} isConnectable={false} aria-hidden="true" />{/if}
  <button type="button" class="card nodrag nopan" class:selected={data.selected} class:dim={data.dim} class:highlighted={data.highlighted}
    aria-label={data.label} aria-pressed={data.selected} data-capability-id={node.id}
    onclick={(event) => { event.stopPropagation(); data.onactivate(); }} onfocus={data.onfocus}>
    <span class="tile" aria-hidden="true">{#if node.kind === "agent"}{monogram(node.label)}{:else}<Icon name={icon} size={18} />{/if}</span>
    <span class="copy"><strong title={node.label}>{node.label}</strong><span class="sub" title={subtitle}>{subtitle}</span><small title={node.emptyHint ?? status}>{node.emptyHint ?? status}</small></span>
    {#if node.kind === "running"}<span class="observed" aria-hidden="true"><Icon name="radar" size={13} /></span>{/if}
  </button>
  {#if node.kind === "agent"}<Handle type="source" id="mcp" position={Position.Left} isConnectable={false} aria-hidden="true" /><Handle type="source" id="skill" position={Position.Right} isConnectable={false} aria-hidden="true" />{/if}
</div>

<style>
  .node-wrap { position: relative; width: var(--node-w); height: var(--node-h); }
  .card { width: 100%; height: 100%; box-sizing: border-box; display: flex; align-items: center; gap: 11px; padding: 14px; border: 1px solid var(--border-strong); border-radius: 15px; background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); font: inherit; text-align: left; cursor: pointer; transition: opacity var(--dur-2) var(--ease), border-color var(--dur-2) var(--ease), box-shadow var(--dur-2) var(--ease); }
  .card:hover { border-color: var(--agent); box-shadow: var(--shadow-md); }
  .card:focus-visible { outline: 2px solid var(--ring); outline-offset: 4px; }
  .card.dim { opacity: .42; }
  .card.highlighted { border-color: var(--agent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 12%, transparent), var(--shadow-sm); }
  .card.selected { opacity: 1; border-color: var(--agent); box-shadow: 0 0 0 2px var(--agent), 0 0 0 6px color-mix(in srgb, var(--agent) 17%, transparent), var(--shadow-md); }
  .tile { width: 34px; height: 34px; flex: none; display: grid; place-items: center; border-radius: 10px; color: var(--agent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .copy { display: grid; gap: 4px; min-width: 0; flex: 1; }
  strong { color: var(--text); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); font-weight: var(--fw-semibold); }
  strong, .sub, small { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .sub, small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); }
  small { color: var(--text-2); }
  .k-agent .card { border-color: color-mix(in srgb, var(--agent) 55%, var(--border-strong)); box-shadow: 0 0 0 4px color-mix(in srgb, var(--agent) 8%, transparent), var(--shadow-sm); }
  .k-agent .tile { width: 40px; height: 40px; border-radius: 12px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); }
  .k-agent .card.highlighted { border-color: var(--agent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--agent) 15%, transparent), var(--shadow-md); }
  .k-agent .card.selected { border-color: var(--agent); box-shadow: 0 0 0 2px var(--agent), 0 0 0 7px color-mix(in srgb, var(--agent) 20%, transparent), var(--shadow-md); }
  .k-skill .tile { color: var(--tone-violet); background: color-mix(in srgb, var(--tone-violet) 12%, var(--surface)); }
  .k-running .card { border-style: dashed; background: var(--surface-2); }
  .k-running .tile { color: var(--text-2); background: var(--surface); }
  .observed { color: var(--muted); display: inline-flex; flex: none; }
  .node-wrap :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .card { transition: none; } }
</style>
