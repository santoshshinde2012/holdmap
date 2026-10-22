<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "../../Icon.svelte";
  import { Select } from "../../ui";
  import { capabilityLabel, capabilityItems, filterCapabilities, projectShort, scopeLabel, type CapabilityContextGroup, type CapabilityItem, type InventoryKind, type InventoryState } from "../../../lib/agent-capabilities";
  import type { CapabilityScope } from "../../../lib/types";

  import type { Snippet } from "svelte";
  let { groups, query, kind, scope, enablement, selectedId, onselect, oncontextchange, detail }: {
    groups: CapabilityContextGroup[]; query: string; kind: InventoryKind | "running" | "tools";
    scope: "all" | CapabilityScope; enablement: InventoryState; selectedId: string | null; onselect: (id: string) => void; oncontextchange: (agentIds: string[]) => void;
    /** The open row's details, rendered inline under it. */
    detail?: Snippet;
  } = $props();
  const uid = $props.id();
  const panelId = uid + "-detail";
  let leftId = $state("");
  let rightId = $state("");
  let leftAnchor = $state("");
  let rightAnchor = $state("");
  const left = $derived(groups.find(group => group.id === leftId) ?? groups.find(group => group.agents.some(agent => agent.id === leftAnchor)) ?? groups[0]);
  const right = $derived(groups.find(group => group.id === rightId && group.id !== left?.id) ?? groups.find(group => group.id !== left?.id && group.agents.some(agent => agent.id === rightAnchor)) ?? groups.find(group => group.id !== left?.id));
  const options = $derived(groups.map(group => ({ value: group.id, label: group.name + " · " + projectShort(group) })));
  $effect(() => {
    if (leftId !== (left?.id ?? "")) leftId = left?.id ?? "";
    if (rightId !== (right?.id ?? "")) rightId = right?.id ?? "";
    if (left && !left.agents.some(agent => agent.id === leftAnchor)) leftAnchor = left.representative.id;
    if (right && !right.agents.some(agent => agent.id === rightAnchor)) rightAnchor = right.representative.id;
  });
  $effect(() => {
    const ids = [...new Set([...(left?.agents ?? []), ...(right?.agents ?? [])].map(agent => agent.id))].sort();
    untrack(() => oncontextchange(ids));
  });
  const rows = $derived.by(() => {
    const all = capabilityItems([...(left?.agents ?? []), ...(right?.agents ?? [])]);
    const filtered = filterCapabilities(all, query, kind === "running" || kind === "tools" ? "all" : kind, scope, enablement);
    const members = new Map<string, { item: CapabilityItem; left: boolean; right: boolean }>();
    for (const item of filtered) members.set(item.id, { item, left: item.owners.some(owner => left?.agents.some(agent => agent.id === owner.id)), right: item.owners.some(owner => right?.agents.some(agent => agent.id === owner.id)) });
    return [...members.values()];
  });
  const coverage = (group: CapabilityContextGroup | undefined) => {
    const seen = group?.agents.filter(agent => agent.capabilities?.inspected).length ?? 0;
    return group && seen === group.agents.length ? "inspected" : seen ? "partly inspected" : "not inspected";
  };
</script>

<section class="comparison" aria-label="Capability comparison">
  <header><div><h3>Compare sources</h3><p>Shared = the same definition found in both.</p></div></header>
  {#if groups.length < 2}
    <div class="empty"><Icon name="layers" size={26} /><h4>One observed context</h4><p>Compare needs two different contexts.</p></div>
  {:else if kind === "running"}
    <div class="empty"><Icon name="cpu" size={26} /><h4>Runtime stays separate</h4><p>Compare covers config files. Use Explore or Map for running processes.</p></div>
  {:else}
    <div class="selectors">
      <Select label="Left context" prefix="Left context" labelHidden size="sm" value={left?.id} options={options} onchange={value => leftId = value} />
      <Select label="Right context" prefix="Right context" labelHidden size="sm" value={right?.id} options={options.filter(option => option.value !== left?.id)} onchange={value => rightId = value} />
    </div>
    <div class="context-notes"><span>{left?.name}: {coverage(left)}</span><span>{right?.name}: {coverage(right)}</span></div>
    <p class="summary" role="status">{rows.length} definitions · {rows.filter(row => row.left && row.right).length} shared</p>
    {#if !rows.length}<div class="empty"><h4>No matches</h4><p>Try another search or filter.</p></div>{/if}
    <div class="rows">
      {#each rows as row (row.item.id)}
        <button class="comparison-row" class:selected={selectedId === row.item.id} data-capability-id={row.item.id} aria-expanded={selectedId === row.item.id} aria-controls={selectedId === row.item.id && detail ? panelId : undefined} aria-label={capabilityLabel(row.item).replace(/^Inspect /, "Inspect compared ")} onclick={() => onselect(row.item.id)}>
          <span class="identity"><b><Icon name={row.item.kind === "skill" ? "sparkles" : "plug"} size={15} />{row.item.name}</b><small>{scopeLabel[row.item.scope]} · {row.item.sourcePath}</small><small>{row.left && row.right ? "In both" : row.left ? "Left only" : "Right only"}</small></span>
          <span class="presence"><small>{left?.name}</small><span>{row.left ? "Found" : "Not found"}</span></span>
          <span class="presence"><small>{right?.name}</small><span>{row.right ? "Found" : "Not found"}</span></span>
        </button>
        {#if detail && selectedId === row.item.id}<div class="inline-detail" id={panelId} role="region" aria-label="Capability details">{@render detail()}</div>{/if}
      {/each}
    </div>
    <div class="note"><Icon name="info" size={15} /><p>“Not found” means not reported, not proof of absence. Same names at different paths stay separate.</p></div>
  {/if}
</section>

<style>
  .comparison { padding: 22px; }
  h3, h4, p { margin: 0; } h3 { font-size: var(--fs-body); line-height: var(--lh-body); } h4 { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  header p, .empty p, .note p { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 7px; }
  .selectors { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin: 20px 0 10px; }
  .selectors :global(.field) { min-width: 0; } .selectors :global(button) { max-width: 100%; } .selectors :global(.val) { overflow: hidden; text-overflow: ellipsis; }
  .context-notes { display: flex; gap: 12px 20px; flex-wrap: wrap; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .summary { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); margin: 18px 0 12px; }
  .rows { display: grid; gap: 9px; }
  .inline-detail { border: 1px solid var(--accent); border-radius: 12px; background: var(--surface); margin-top: -4px; }
  .comparison-row { display: grid; grid-template-columns: minmax(0, 1.7fr) minmax(0, 1fr) minmax(0, 1fr); gap: 14px; width: 100%; border: 1px solid var(--border); border-radius: 12px; padding: 15px; text-align: left; background: var(--surface); color: var(--text-2); font: inherit; cursor: pointer; }
  .comparison-row:hover { background: var(--row-hover); border-color: var(--border-strong); } .comparison-row.selected { border-color: var(--accent); }
  .comparison-row:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .identity, .presence { display: flex; flex-direction: column; gap: 7px; min-width: 0; overflow-wrap: anywhere; }
  .identity b { display: flex; align-items: center; gap: 7px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .identity :global(svg) { color: var(--accent); }
  small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .identity small { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .presence > span { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .empty { display: grid; gap: 8px; justify-items: start; padding: 25px; margin-top: 20px; background: var(--surface); border: 1px solid var(--border); border-radius: 13px; color: var(--muted); }
  .empty h4 { color: var(--text-2); } .empty p { max-width: 500px; }
  .note { display: flex; align-items: flex-start; gap: 8px; margin-top: 20px; color: var(--muted); }.note :global(svg) { margin-top: 2px; }.note p { margin: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  @container (max-width: 700px) { .comparison { padding: 15px; } .selectors { grid-template-columns: 1fr; } .comparison-row { grid-template-columns: 1fr 1fr; } .identity { grid-column: 1 / -1; } }
</style>
