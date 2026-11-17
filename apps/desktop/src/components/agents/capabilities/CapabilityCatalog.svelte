<script lang="ts">
  import Icon from "../../Icon.svelte";
  import { capabilityLabel, enabledLabel, scopeLabel, type CapabilityItem, type RunningMcpItem } from "../../../lib/agent-capabilities";
  import type { AgentSkill } from "../../../lib/types";
  import FindingsBadge from "../../analyzers/FindingsBadge.svelte";
  import { insightsStore } from "../../../lib/insights-store.svelte";
  import { subjectCounts } from "../../../lib/analyzers";

  // Cards select a capability. The dossier opens in the workspace sidebar (#capability-details).
  let { items, running, selectedId, onselect }: {
    items: CapabilityItem[]; running: RunningMcpItem[]; selectedId: string | null; onselect: (id: string) => void;
  } = $props();
  const panelId = "capability-details";
  let copied = $state<string | null>(null);
  async function copy(path: string) {
    try { await navigator.clipboard.writeText(path); copied = path; setTimeout(() => { if (copied === path) copied = null; }, 1600); } catch { copied = null; }
  }
</script>

{#snippet card(item: CapabilityItem)}
  <div class="cap kind-{item.kind}">
    <button class="capability-card" class:selected={selectedId === item.id} aria-expanded={selectedId === item.id} aria-controls={selectedId === item.id ? panelId : undefined} aria-label={capabilityLabel(item)} data-capability-id={item.id} onclick={() => onselect(item.id)}>
      <span class="card-title"><span class="symbol"><Icon name={item.kind === "skill" ? "sparkles" : "plug"} size={16} /></span><b>{item.name}</b>{#if item.kind === "skill"}<FindingsBadge counts={subjectCounts(insightsStore.insights, item.definition.id)} />{/if}<Icon name={selectedId === item.id ? "panel" : "chevron-down"} size={14} /></span>
      <span class="purpose">{item.kind === "skill" ? (item.definition as AgentSkill).description || "No description." : "MCP server definition. Tools not reported."}</span>
      <span class="metadata"><span class="evidence">{item.kind === "skill" ? "Skill file found" : "MCP definition found"}</span><span>{scopeLabel[item.scope]}</span>{#if item.kind === "mcp"}<span>{enabledLabel(item)}</span>{/if}</span>
      <span class="owners">{[...new Set(item.owners.map(owner => owner.name))].join(" · ")} · {item.owners.length} {item.owners.length === 1 ? "process" : "processes"}</span>
      <span class="source" title={item.sourcePath}>{item.sourcePath}</span>
    </button>
    <button type="button" class="copy" aria-label={copied === item.sourcePath ? "Copied" : `Copy path of ${item.name}`} onclick={() => copy(item.sourcePath)}><Icon name={copied === item.sourcePath ? "check" : "copy"} size={11} />{copied === item.sourcePath ? "Copied" : "Copy path"}</button>
  </div>
{/snippet}

<div class="catalog" class:shelves={items.some((item) => item.kind === "skill") && items.some((item) => item.kind === "mcp")} role="region" aria-label="Capability catalog">
  {#if items.some((item) => item.kind === "skill") && items.some((item) => item.kind === "mcp")}
    <h3 class="shelf">Skill files <span>{items.filter((item) => item.kind === "skill").length}</span></h3>
    {#each items.filter((item) => item.kind === "skill") as item (item.id)}{@render card(item)}{/each}
    <h3 class="shelf">MCP definitions <span>{items.filter((item) => item.kind === "mcp").length}</span></h3>
    {#each items.filter((item) => item.kind === "mcp") as item (item.id)}{@render card(item)}{/each}
  {:else}
    {#each items as item (item.id)}{@render card(item)}{/each}
  {/if}
  {#if running.length && items.length}<h3 class="shelf">Running candidates <span>{running.length}</span></h3>{/if}
  {#each running as item (item.id)}
    <div class="cap kind-running">
      <button class="capability-card" class:selected={selectedId === item.id} aria-expanded={selectedId === item.id} aria-controls={selectedId === item.id ? panelId : undefined} aria-label={`Inspect running process ${item.name}, PID ${item.tool.pid}`} data-capability-id={item.id} onclick={() => onselect(item.id)}>
        <span class="card-title"><span class="symbol"><Icon name="cpu" size={16} /></span><b>{item.name}</b><Icon name={selectedId === item.id ? "panel" : "chevron-down"} size={14} /></span>
        <span class="purpose">Likely MCP server process. Connection not confirmed.</span>
        <span class="metadata"><span class="evidence">Process observed</span><span>Runtime candidate</span></span>
        <span class="owners">Separate from configuration and recorded calls</span>
        <span class="source">PID {item.tool.pid}{item.tool.cwd ? " · " + item.tool.cwd : ""}</span>
      </button>
    </div>
  {/each}
</div>

<style>
  .catalog { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr)); gap: 12px; align-content: start; align-items: stretch; grid-auto-flow: row dense; padding: 16px; }
  .catalog.shelves { grid-auto-flow: row; }
  .shelf { grid-column: 1 / -1; display: flex; align-items: center; gap: 8px; margin: 6px 0 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  .shelf::after { content: ""; flex: 1; height: 1px; background: var(--border); }
  .shelf span { display: inline-grid; place-items: center; min-width: 18px; height: 18px; padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); }
  .shelf + .shelf, .cap + .shelf { margin-top: 10px; }
  .cap { --k: var(--accent); position: relative; display: flex; min-width: 0; height: 100%; }
  .kind-skill { --k: var(--tone-amber); }
  .kind-running { --k: var(--tone-green); }
  .capability-card { display: flex; flex-direction: column; gap: 8px; width: 100%; min-width: 0; padding: 14px 14px 12px; border: 1px solid var(--border); border-radius: 14px; background: linear-gradient(180deg, color-mix(in srgb, var(--k) 9%, var(--surface)) 0, var(--surface) 48px); color: var(--text-2); text-align: left; font: inherit; cursor: pointer; box-shadow: var(--shadow-sm); }
  .capability-card:hover { border-color: var(--border-strong); }
  .capability-card.selected { border-color: color-mix(in srgb, var(--k) 65%, var(--border)); box-shadow: 0 0 0 1px color-mix(in srgb, var(--k) 45%, transparent); }
  .capability-card:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .card-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .card-title b { flex: 1; min-width: 0; font-size: var(--fs-body); line-height: var(--lh-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card-title :global(.badge) { flex: none; }
  .card-title > :global(svg) { color: var(--muted); flex: none; }
  .symbol { display: grid; place-items: center; width: 28px; height: 28px; flex: none; border-radius: 9px; background: color-mix(in srgb, var(--k) 14%, var(--surface)); color: var(--k); }
  .purpose { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; min-height: calc(var(--lh-body-sm) * 2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); }
  .metadata { display: flex; gap: 0; flex-wrap: wrap; align-items: center; margin-top: auto; }
  .metadata span { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .metadata span + span::before { content: "·"; margin: 0 6px; color: var(--faint, var(--muted)); }
  .metadata .evidence { color: var(--k); font-weight: var(--fw-medium); }
  .owners { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .source { min-width: 0; margin-top: 2px; padding: 8px 88px 0 0; border-top: 1px solid var(--border); color: var(--muted); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kind-running .source { padding-right: 0; }
  .copy { position: absolute; right: 10px; bottom: 8px; display: inline-flex; align-items: center; gap: 4px; min-height: 22px; padding: 0 2px; border: 0; background: transparent; color: var(--muted); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .copy:hover { color: var(--text); }
  .copy:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
</style>
