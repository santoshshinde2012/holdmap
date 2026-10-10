<script lang="ts">
  // A folder, port, service or remote host in an agent's footprint. Dots show which agents
  // touch it; recent projects and inferred paths are drawn quieter (dashed).
  import { Handle, Position, type NodeProps, type Node } from "@xyflow/svelte";
  import Icon from "../Icon.svelte";
  import type { FootFlowData } from "../../lib/agents";
  import { EVIDENCE_LABEL } from "../../lib/agents";

  let { data }: NodeProps<Node<FootFlowData>> = $props();
  const n = $derived(data.node);
  const ICON = { folder: "folder", tool: "plug", port: "server", service: "plug", remote: "globe", more: "more", omitted: "more", agent: "bot", header: "" } as const;
  const WHAT = { folder: "Folder", tool: "MCP server", port: "Port", service: "Service", remote: "Remote host", more: "More remote links", omitted: "Unlisted links", agent: "Agent", header: "" } as const;
  const opens = $derived(!!n.entryId);
  const interactive = $derived(opens || n.kind === "tool");
  const title = $derived(n.path ?? n.warning ?? undefined);
</script>

<div
  class="node k-{n.kind}"
  class:muted={n.tone === "muted"}
  class:warn={n.tone === "warn"}
  class:inferred={n.evidence !== "observed"}
  class:dim={data.dim}
  class:hl={data.hl}
  class:sel={data.selected}
  class:opens={interactive}
  style="--agent: {data.color}"
  role={interactive ? "button" : "group"}
  aria-label="{WHAT[n.kind]} {n.label}, {n.sub}{n.evidence !== 'observed' ? `, ${EVIDENCE_LABEL[n.evidence]}` : ''}{n.warning ? `, ${n.warning}` : ''}{opens ? '. Opens details' : n.kind === 'tool' ? '. Opens its agent card' : ''}"
  {title}
>
  <Handle type="target" position={Position.Left} isConnectable={false} />
  <span class="ic" aria-hidden="true"><Icon name={ICON[n.kind]} size={15} /></span>
  <div class="body">
    <span class="label">{n.label}</span>
    <span class="sub" class:mono={n.kind === "port" || n.kind === "service"}>{n.sub}{#if n.evidence !== "observed"}<em> · {EVIDENCE_LABEL[n.evidence]}</em>{/if}</span>
  </div>
  {#if n.owners.length > 1}
    <span class="dots" aria-hidden="true">{#each data.ownerColors.slice(0, 4) as c}<i style="background: {c}"></i>{/each}</span>
  {/if}
  {#if n.tone === "warn"}<span class="exp" title={n.warning}><Icon name="globe" size={12} /></span>{/if}
  <Handle type="source" position={Position.Right} isConnectable={false} />
</div>

<style>
  .node {
    width: 216px; height: 56px; box-sizing: border-box; display: flex; align-items: center; gap: 10px;
    padding: 0 12px; border-radius: 12px; background: var(--surface); border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-sm); color: var(--text);
    transition: opacity var(--dur-2) var(--ease), box-shadow var(--dur-2) var(--ease), border-color var(--dur-2) var(--ease);
  }
  .node.opens { cursor: pointer; }
  .node.opens:hover { border-color: var(--accent); box-shadow: var(--shadow-md); }
  .node.muted, .node.inferred { border-style: dashed; background: var(--surface-2); }
  .k-remote, .k-more { background: var(--surface-2); }
  .node.warn { border-color: color-mix(in srgb, var(--warn) 60%, var(--border-strong)); }
  .node.dim { opacity: 0.45; filter: saturate(0.3); }
  .node.hl { border-color: var(--agent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 18%, transparent), var(--shadow-md); }
  .node.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent), 0 0 0 6px var(--accent-soft), var(--shadow-md); }
  .ic { width: 30px; height: 30px; flex: none; display: grid; place-items: center; border-radius: 9px; background: var(--surface-2); color: var(--text-2); }
  .k-folder .ic { color: var(--tone-amber); background: color-mix(in srgb, var(--tone-amber) 12%, var(--surface)); }
  .k-port .ic { color: var(--tone-green); background: color-mix(in srgb, var(--tone-green) 12%, var(--surface)); }
  .k-service .ic { color: var(--tone-violet); background: color-mix(in srgb, var(--tone-violet) 12%, var(--surface)); }
  .k-tool .ic { color: var(--tone-violet); background: color-mix(in srgb, var(--tone-violet) 12%, var(--surface)); }
  .k-remote .ic, .k-more .ic { color: var(--tone-blue); background: color-mix(in srgb, var(--tone-blue) 12%, var(--surface)); }
  .muted .ic { opacity: 0.75; }
  .body { flex: 1; min-width: 0; display: grid; gap: 1px; }
  .label { font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muted .label { font-weight: var(--fw-medium); color: var(--text-2); }
  .sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub.mono { font-family: var(--mono); }
  .sub em { font-style: normal; color: var(--warn); font-family: var(--font); }
  .dots { display: inline-flex; gap: 3px; }
  .dots i { width: 7px; height: 7px; border-radius: 50%; }
  .exp { color: var(--warn); display: inline-flex; }
  .node :global(.svelte-flow__handle) { opacity: 0; pointer-events: none; }
  @media (prefers-reduced-motion: reduce) { .node { transition: none; } }
</style>
