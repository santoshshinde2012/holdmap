<script lang="ts">
  // An agent in the footprint graph: monogram in the agent's colour, name, kind and pid, and the
  // access headline (sandboxed, your account, root…).
  import { Handle, Position, type NodeProps, type Node } from "@xyflow/svelte";
  import { accessHeadline, counts, countsLine, monogram, resourcesLine, type FootFlowData } from "../../lib/agents";

  let { data }: NodeProps<Node<FootFlowData>> = $props();
  const a = $derived(data.node.agent!);
  const head = $derived(accessHeadline(a));
  const c = $derived(counts(a));
  const res = $derived(resourcesLine(a));
</script>

<div
  class="node"
  class:dim={data.dim}
  class:hl={data.hl}
  class:sel={data.selected}
  style="--agent: {data.color}"
  role="button"
  aria-label="{a.name}, {data.node.sub}. {head.text}. {countsLine(c)}. {res}"
>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <span class="mono-tile" aria-hidden="true">{monogram(a.name)}</span>
  <div class="body">
    <span class="label">{a.name}{#if c.stoppable > 0}<em class="stop-n">{c.stoppable}</em>{/if}</span>
    <span class="sub">{data.node.sub}</span>
    <span class="acc lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.text} · {res.split(" · ").slice(0, 2).join(" · ")}</span>
  </div>
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>

<style>
  .node {
    width: 248px; height: 84px; box-sizing: border-box; display: flex; align-items: center; gap: 12px;
    padding: 0 14px; border-radius: 16px; background: var(--surface); color: var(--text); cursor: pointer;
    border: 1px solid color-mix(in srgb, var(--agent) 55%, var(--border-strong));
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--agent) 10%, transparent), var(--shadow-sm);
    transition: opacity var(--dur-2) var(--ease), box-shadow var(--dur-2) var(--ease), border-color var(--dur-2) var(--ease);
  }
  .node:hover { box-shadow: 0 0 0 4px color-mix(in srgb, var(--agent) 18%, transparent), var(--shadow-md); }
  .node.dim { opacity: 0.5; filter: saturate(0.3); }
  .node.hl { border-color: var(--agent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--agent) 22%, transparent), var(--shadow-md); }
  .node.sel { border-color: var(--agent); box-shadow: 0 0 0 2px var(--agent), 0 0 0 7px color-mix(in srgb, var(--agent) 20%, transparent), var(--shadow-md); }
  .mono-tile {
    width: 40px; height: 40px; flex: none; display: grid; place-items: center; border-radius: 12px;
    background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent);
    font-family: var(--font); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); letter-spacing: var(--ls-body);
  }
  .body { min-width: 0; display: grid; gap: 1px; }
  .label { font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: inline-flex; align-items: center; gap: 6px; }
  .stop-n { font-style: normal; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--danger); background: color-mix(in srgb, var(--danger) 12%, transparent); padding: 0 6px; border-radius: 999px; }
  .sub, .acc { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acc { display: inline-flex; align-items: center; gap: 5px; color: var(--text-2); }
  .acc i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); flex: none; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .lv-elevated { color: var(--danger); }
  .node :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .node { transition: none; } }
</style>
