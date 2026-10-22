<script lang="ts">
  import Icon from "../../Icon.svelte";
  import { capabilityLabel, enabledLabel, scopeLabel, type CapabilityItem, type RunningMcpItem } from "../../../lib/agent-capabilities";
  import type { AgentSkill } from "../../../lib/types";
  import type { Snippet } from "svelte";
  import FindingsBadge from "../../analyzers/FindingsBadge.svelte";
  import { insightsStore } from "../../../lib/insights-store.svelte";
  import { subjectCounts } from "../../../lib/analyzers";

  // Cards are disclosure buttons: the selected card's details open inline, spanning the row
  // right under it (grid-auto-flow: dense backfills the row), so the eye and scroll stay put.
  let { items, running, selectedId, onselect, detail }: {
    items: CapabilityItem[]; running: RunningMcpItem[]; selectedId: string | null; onselect: (id: string) => void;
    /** The open card's details, rendered inline after it. */
    detail?: Snippet;
  } = $props();
  const uid = $props.id();
  const panelId = uid + "-detail";
</script>

{#snippet inline(id: string)}
  {#if detail && selectedId === id}<div class="inline-detail" id={panelId} role="region" aria-label="Capability details">{@render detail()}</div>{/if}
{/snippet}

<div class="catalog" role="region" aria-label="Capability catalog">
  {#each items as item (item.id)}
    <button class="capability-card kind-{item.kind}" class:selected={selectedId === item.id} aria-expanded={selectedId === item.id} aria-controls={selectedId === item.id ? panelId : undefined} aria-label={capabilityLabel(item)} data-capability-id={item.id} onclick={() => onselect(item.id)}>
      <span class="card-title"><span class="symbol"><Icon name={item.kind === "skill" ? "sparkles" : "plug"} size={19} /></span><b>{item.name}</b>{#if item.kind === "skill"}<FindingsBadge counts={subjectCounts(insightsStore.insights, item.definition.id)} />{/if}<Icon name={selectedId === item.id ? "chevron-up" : "chevron-down"} size={14} /></span>
      <span class="purpose">{item.kind === "skill" ? (item.definition as AgentSkill).description || "No description." : "MCP server definition. Tools not reported."}</span>
      <span class="metadata"><span class="evidence">{item.kind === "skill" ? "Skill file found" : "MCP definition found"}</span><span>{scopeLabel[item.scope]}</span>{#if item.kind === "mcp"}<span>{enabledLabel(item)}</span>{/if}</span>
      <span class="source">{item.sourcePath}</span>
      <span class="owners">{[...new Set(item.owners.map(owner => owner.name))].join(" · ")} · {item.owners.length} {item.owners.length === 1 ? "process" : "processes"}</span>
    </button>
    {@render inline(item.id)}
  {/each}
  {#each running as item (item.id)}
    <button class="capability-card kind-running" class:selected={selectedId === item.id} aria-expanded={selectedId === item.id} aria-controls={selectedId === item.id ? panelId : undefined} aria-label={`Inspect running process ${item.name}, PID ${item.tool.pid}`} data-capability-id={item.id} onclick={() => onselect(item.id)}>
      <span class="card-title"><span class="symbol"><Icon name="cpu" size={19} /></span><b>{item.name}</b><Icon name={selectedId === item.id ? "chevron-up" : "chevron-down"} size={14} /></span>
      <span class="purpose">Likely MCP server process. Connection not confirmed.</span>
      <span class="metadata"><span class="evidence">Process observed</span><span>Runtime candidate</span></span>
      <span class="source">PID {item.tool.pid}{item.tool.cwd ? " · " + item.tool.cwd : ""}</span>
      <span class="owners">Separate from configuration and recorded calls</span>
    </button>
    {@render inline(item.id)}
  {/each}
</div>

<style>
  .catalog { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 285px), 1fr)); gap: 14px; align-content: start; grid-auto-flow: row dense; padding: 18px; }
  .inline-detail { grid-column: 1 / -1; border: 1px solid var(--accent); border-radius: 15px; background: var(--surface); box-shadow: var(--shadow-sm); scroll-margin: 18px; }
  .capability-card { display: flex; flex-direction: column; gap: 13px; min-width: 0; padding: 18px; border: 1px solid var(--border); border-radius: 15px; background: var(--surface); color: var(--text-2); text-align: left; font: inherit; cursor: pointer; box-shadow: var(--shadow-sm); }
  .capability-card:hover { border-color: var(--border-strong); background: var(--row-hover); }
  .capability-card.selected { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
  .capability-card:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .card-title { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .card-title b { flex: 1; font-size: var(--fs-body); line-height: var(--lh-body); overflow-wrap: anywhere; }
  .card-title > :global(svg) { color: var(--muted); }
  .symbol { display: grid; place-items: center; width: 34px; height: 34px; flex: none; border-radius: 11px; background: var(--accent-soft); color: var(--accent); }
  .kind-skill .symbol { background: color-mix(in srgb, var(--tone-amber) 12%, var(--surface)); color: var(--tone-amber); }
  .kind-running .symbol { background: color-mix(in srgb, var(--tone-green) 12%, var(--surface)); color: var(--tone-green); }
  .purpose { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; min-height: 3lh; }
  .metadata { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; margin-top: auto; }
  .metadata span { padding: 3px 6px; background: var(--surface-2); border-radius: 5px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .metadata .evidence { background: var(--surface); border: 1px solid var(--border); color: var(--text-2); }
  .source { color: var(--muted); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow-wrap: anywhere; }
  .owners { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
</style>
