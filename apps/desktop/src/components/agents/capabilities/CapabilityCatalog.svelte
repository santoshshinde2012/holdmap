<script lang="ts">
  import Icon from "../../Icon.svelte";
  import { capabilityLabel, enabledLabel, scopeLabel, type CapabilityItem, type RunningMcpItem } from "../../../lib/agent-capabilities";
  import type { AgentSkill } from "../../../lib/types";

  let { items, running, selectedId, onselect }: {
    items: CapabilityItem[]; running: RunningMcpItem[]; selectedId: string | null; onselect: (id: string) => void;
  } = $props();
</script>

<div class="catalog" role="region" aria-label="Capability catalog">
  {#each items as item (item.id)}
    <button class="capability-card kind-{item.kind}" class:selected={selectedId === item.id} aria-pressed={selectedId === item.id} aria-label={capabilityLabel(item)} data-capability-id={item.id} onclick={() => onselect(item.id)}>
      <span class="card-title"><span class="symbol"><Icon name={item.kind === "skill" ? "sparkles" : "plug"} size={19} /></span><b>{item.name}</b><Icon name="chevron" size={14} /></span>
      <span class="purpose">{item.kind === "skill" ? (item.definition as AgentSkill).description || "Purpose not reported. Inspect the source and evidence to learn more about this file." : "A configured MCP server definition. Its purpose and individual tools have not been reported."}</span>
      <span class="metadata"><span class="evidence">{item.kind === "skill" ? "Skill file found" : "MCP definition found"}</span><span>{scopeLabel[item.scope]}</span>{#if item.kind === "mcp"}<span>{enabledLabel(item)}</span>{/if}</span>
      <span class="source">{item.sourcePath}</span>
      <span class="owners">{[...new Set(item.owners.map(owner => owner.name))].join(" · ")} · {item.owners.length} observed {item.owners.length === 1 ? "process" : "processes"}</span>
    </button>
  {/each}
  {#each running as item (item.id)}
    <button class="capability-card kind-running" class:selected={selectedId === item.id} aria-pressed={selectedId === item.id} aria-label={`Inspect running process ${item.name}, PID ${item.tool.pid}`} data-capability-id={item.id} onclick={() => onselect(item.id)}>
      <span class="card-title"><span class="symbol"><Icon name="cpu" size={19} /></span><b>{item.name}</b><Icon name="chevron" size={14} /></span>
      <span class="purpose">An observed MCP candidate process. Connection and callable tools have not been established.</span>
      <span class="metadata"><span class="evidence">Process observed</span><span>Runtime candidate</span></span>
      <span class="source">PID {item.tool.pid}{item.tool.cwd ? " · " + item.tool.cwd : ""}</span>
      <span class="owners">Separate from configuration and recorded calls</span>
    </button>
  {/each}
</div>

<style>
  .catalog { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 285px), 1fr)); gap: 14px; align-content: start; padding: 18px; }
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
