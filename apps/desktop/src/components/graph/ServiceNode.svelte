<script lang="ts">
  // A service in the topology graph: monogram, name, ports and live resource usage.
  import { Handle, Position, type NodeProps, type Node } from "@xyflow/svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import Icon from "../Icon.svelte";
  import { nodeSubtitle, serviceLabel, type FlowNodeData } from "../../lib/graph";
  import { humanBytes as bytes } from "../../lib/format";
  import { getContext } from "svelte";

  let { data }: NodeProps<Node<FlowNodeData>> = $props();
  const n = $derived(data.node!);
  const sub = $derived(nodeSubtitle(n));
  const exposed = $derived(n.ports.some((p) => p.exposure === "all_interfaces"));
  const usage = $derived(n.kind !== "external" && n.pids.length ? { cpu: `${n.cpu_percent.toFixed(n.cpu_percent < 10 ? 1 : 0)}%`, memory: bytes(n.memory_bytes) } : null);
  const actions = getContext<{ select: (node: typeof n) => void; focus: (id: string) => void }>("service-graph-actions");
</script>

<div class="node-wrap">
  <Handle type="target" position={Position.Left} isConnectable={false} aria-hidden="true" />
  <button
  type="button"
  class="node nodrag nopan kind-{n.kind}"
  class:dim={data.dim}
  class:hl={data.hl}
  class:sel={data.selected}
  class:dev={n.is_dev}
  data-service-id={n.id}
  aria-pressed={data.selected}
  aria-label={serviceLabel(n, usage)}
  onclick={(event) => { event.stopPropagation(); actions.select(n); }}
  onfocus={(event) => { if ((event.currentTarget as HTMLElement).matches(":focus-visible")) actions.focus(n.id); }}
>
  <FrameworkIcon node={n} size={34} />
  <span class="body">
    <span class="top">
      <span class="label">{n.label}</span>
      {#if n.protected}<span class="lock" title="Protected"><Icon name="lock" size={11} /></span>{/if}
      {#if exposed}<span class="exp" title="Reachable from the network"><Icon name="globe" size={11} /></span>{/if}
    </span>
    <span class="ports">
      {#each n.ports.slice(0, 3) as p}<span class="port mono">:{p.port}</span>{/each}
      {#if n.ports.length > 3}<span class="more">+{n.ports.length - 3}</span>{/if}
      {#if sub}<span class="sub">{sub}</span>{/if}
    </span>
  </span>
  {#if usage}
    <span class="usage mono" title="CPU · memory of the whole process tree">
      <span data-glyph={usage.cpu}></span>
      <span data-glyph={usage.memory}></span>
    </span>
  {/if}
  </button>
  <Handle type="source" position={Position.Right} isConnectable={false} aria-hidden="true" />
</div>

<style>
  .node-wrap { position: relative; width: 236px; height: 72px; }
  .node {
    width: 236px; height: 72px; box-sizing: border-box; display: flex; align-items: center; gap: 10px;
    padding: 0 12px; border-radius: 14px; background: var(--surface); border: 1px solid var(--border-strong);
    box-shadow: var(--shadow); color: var(--text); cursor: pointer;
    font: inherit; text-align: left;
    transition: opacity var(--dur-2) var(--ease), box-shadow var(--dur-2) var(--ease), border-color var(--dur-2) var(--ease), transform var(--dur-2) var(--ease);
  }
  .node:hover { box-shadow: var(--shadow-md); border-color: var(--accent); }
  .node:focus-visible { outline: 2px solid var(--ring); outline-offset: 4px; }
  .node.dev { border-left: 3px solid var(--tone-green); }
  .node.kind-external { background: var(--surface-2); border-style: dashed; }
  .node.kind-client { border-style: dashed; }
  .node.kind-hidden { opacity: 0.8; }
  /* Keep unrelated services readable; soften their chrome without fading the text. */
  .node.dim { opacity: 1; filter: saturate(0.3); border-color: var(--border); box-shadow: var(--shadow-sm); }
  .node.hl { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft), var(--shadow-md); }
  .node.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent), 0 0 0 6px var(--accent-soft), var(--shadow-md); }
  .body { flex: 1; min-width: 0; display: grid; gap: 4px; }
  .top { display: flex; align-items: center; gap: 5px; min-width: 0; }
  .label { font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lock, .exp { display: inline-flex; align-items: center; padding: 2px; border-radius: 5px; background: var(--surface-2); }
  .lock { color: var(--muted); }
  .exp { color: var(--warn); background: var(--warn-soft); }
  .ports { display: flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .port { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--accent); background: var(--accent-soft); padding: 1px 5px; border-radius: 5px; }
  .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
  .usage { display: grid; flex: none; justify-items: end; gap: 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .node-wrap :global(.svelte-flow__handle) { width: 7px; height: 7px; background: var(--border-strong); border: 2px solid var(--surface); opacity: 0.9; }
  @media (prefers-reduced-motion: reduce) { .node { transition: none; } }
</style>
