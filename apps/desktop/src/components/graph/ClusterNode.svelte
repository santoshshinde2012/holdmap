<script lang="ts">
  // Cluster hull: a translucent group behind its member services, with a stop-cluster action.
  import type { NodeProps, Node } from "@xyflow/svelte";
  import Icon from "../Icon.svelte";
  import { CLUSTER_LABEL, type FlowNodeData } from "../../lib/graph";
  import { getContext } from "svelte";

  let { data, width, height }: NodeProps<Node<FlowNodeData>> = $props();
  const c = $derived(data.cluster!);
  const actions = getContext<{ stopCluster: (name: string) => void } | undefined>("graph-actions");
  const icon: Record<string, string> = { compose: "box", kubernetes: "radar", supervisor: "layers", workspace: "folder", git: "branch" };
</script>

<div class="hull kind-{c.kind}" class:dim={data.dim} class:narrow={(width ?? 0) < 420} style="width: {width}px; height: {height}px">
  <header title="{CLUSTER_LABEL[c.kind]}{c.detail ? ` · ${c.detail}` : ''}{c.root ? ` · ${c.root}` : ''}">
    <span class="ic"><Icon name={icon[c.kind] ?? "layers"} size={12} /></span>
    <span class="name">{c.name}</span>
    <span class="kind">{CLUSTER_LABEL[c.kind]}{c.detail && !c.name.includes(c.detail) ? ` · ${c.detail}` : ""}</span>
    <span class="count">{data.count}</span>
    {#if actions}
      <button class="stop nodrag" aria-label="Stop cluster {c.name}" title="Stop every service in {c.name}, dependents first" onclick={(e) => { e.stopPropagation(); actions.stopCluster(c.name); }}>
        <Icon name="stop" size={10} /><span class="sl">Stop cluster</span>
      </button>
    {/if}
  </header>
</div>

<style>
  .hull {
    --hue: var(--accent);
    box-sizing: border-box; border-radius: 20px; border: 1.5px dashed color-mix(in srgb, var(--hue) 45%, transparent);
    background: color-mix(in srgb, var(--hue) 6%, transparent); transition: opacity 160ms var(--ease);
  }
  .kind-compose { --hue: var(--tone-blue); }
  .kind-kubernetes { --hue: var(--tone-violet); }
  .kind-supervisor { --hue: var(--tone-amber); }
  .kind-workspace { --hue: var(--tone-green); }
  .kind-git { --hue: var(--tone-gray); }
  .hull.dim { opacity: 0.35; }
  header { display: flex; align-items: center; gap: 7px; height: 34px; padding: 0 10px 0 12px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .ic { display: inline-grid; place-items: center; width: 20px; height: 20px; border-radius: 6px; color: var(--hue); background: color-mix(in srgb, var(--hue) 14%, transparent); }
  .name { font-weight: var(--fw-semibold); color: var(--text); letter-spacing: var(--ls-heading); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .kind { color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 1; }
  .narrow .kind { display: none; }
  .narrow .name { flex: 1; }
  .narrow .sl { display: none; }
  .count { flex: none; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--muted); background: var(--surface-3); border-radius: 999px; padding: 0 6px; }
  .stop {
    flex: none; display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px; border-radius: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium);
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text-2); cursor: pointer; opacity: 0.85;
  }
  .stop:hover { color: var(--danger); border-color: var(--danger); opacity: 1; }
</style>
