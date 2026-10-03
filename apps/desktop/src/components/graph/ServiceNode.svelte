<script lang="ts">
  // A service in the topology graph: monogram, name, ports and live resource usage.
  import { Handle, Position, type NodeProps, type Node } from "@xyflow/svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import Icon from "../Icon.svelte";
  import type { FlowNodeData } from "../../lib/graph";
  import { humanBytes as bytes } from "../../lib/format";

  let { data }: NodeProps<Node<FlowNodeData>> = $props();
  const n = $derived(data.node!);
  const sub = $derived(
    n.kind === "external"
      ? "Internet & remote hosts"
      : [n.framework?.name, n.container ? n.container.image : null, n.kind === "client" ? "client" : null].filter(Boolean).join(" · ") || n.subtitle || ""
  );
  const exposed = $derived(n.ports.some((p) => p.exposure === "all_interfaces"));
</script>

<div
  class="node kind-{n.kind}"
  class:dim={data.dim}
  class:hl={data.hl}
  class:sel={data.selected}
  class:dev={n.is_dev}
  role="button"
  aria-label="{n.label}{n.ports.length ? ' on ports ' + n.ports.map((p) => p.port).join(', ') : ''}"
>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <FrameworkIcon node={n} size={34} />
  <div class="body">
    <div class="top">
      <span class="label">{n.label}</span>
      {#if n.protected}<span class="lock" title="Protected"><Icon name="lock" size={11} /></span>{/if}
      {#if exposed}<span class="exp" title="Reachable from the network"><Icon name="globe" size={11} /></span>{/if}
    </div>
    <div class="ports">
      {#each n.ports.slice(0, 3) as p}<span class="port mono">:{p.port}</span>{/each}
      {#if n.ports.length > 3}<span class="more">+{n.ports.length - 3}</span>{/if}
      {#if sub}<span class="sub">{sub}</span>{/if}
    </div>
  </div>
  {#if n.kind !== "external" && n.pids.length}
    <div class="usage mono" title="CPU · memory of the whole process tree">
      <span>{n.cpu_percent.toFixed(n.cpu_percent < 10 ? 1 : 0)}%</span>
      <span>{bytes(n.memory_bytes)}</span>
    </div>
  {/if}
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>

<style>
  .node {
    width: 236px; height: 72px; box-sizing: border-box; display: flex; align-items: center; gap: 10px;
    padding: 0 12px; border-radius: 14px; background: var(--surface); border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-sm); color: var(--text); cursor: pointer;
    transition: opacity 160ms var(--ease), box-shadow 160ms var(--ease), border-color 160ms var(--ease), transform 160ms var(--ease);
  }
  .node:hover { box-shadow: var(--shadow-md); border-color: var(--accent); }
  .node.dev { border-left: 3px solid var(--tone-green); }
  .node.kind-external { background: var(--surface-2); border-style: dashed; }
  .node.kind-client { border-style: dashed; }
  .node.kind-hidden { opacity: 0.8; }
  .node.dim { opacity: 0.28; }
  .node.hl { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft), var(--shadow-md); }
  .node.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent), 0 0 0 6px var(--accent-soft), var(--shadow-md); }
  .body { flex: 1; min-width: 0; display: grid; gap: 4px; }
  .top { display: flex; align-items: center; gap: 5px; min-width: 0; }
  .label { font-weight: 650; font-size: 13px; letter-spacing: -0.01em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lock { color: var(--muted); display: inline-flex; }
  .exp { color: var(--warn); display: inline-flex; }
  .ports { display: flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .port { font-size: 10.5px; font-weight: 650; color: var(--accent); background: var(--accent-soft); padding: 1px 5px; border-radius: 5px; }
  .more { font-size: 10px; color: var(--muted); }
  .sub { font-size: 10.5px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
  .usage { display: grid; justify-items: end; gap: 2px; font-size: 10px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .node :global(.svelte-flow__handle) { width: 7px; height: 7px; background: var(--border-strong); border: 2px solid var(--surface); opacity: 0.9; }
  @media (prefers-reduced-motion: reduce) { .node { transition: none; } }
</style>
