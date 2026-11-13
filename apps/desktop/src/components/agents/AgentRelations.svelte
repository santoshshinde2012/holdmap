<script lang="ts">
  // "Started from X" / "Started Y, Z": one agent's parent and child agents, shared by the rail
  // card and the details pane. Each name jumps to that agent when navigation is offered.
  import { startedFrom } from "../../lib/agents";
  import type { Agent, AgentsReport } from "../../lib/types";

  let { agent, report, onnavigate }: { agent: Agent; report: AgentsReport; onnavigate?: (id: string) => void } = $props();
  const parent = $derived(report.agents.find((a) => a.id === agent.parent) ?? null);
  const children = $derived(startedFrom(report, agent.id));
</script>

{#snippet who(a: Agent)}{#if onnavigate}<button type="button" onclick={() => onnavigate?.(a.id)}>{a.name} <span class="mono">{a.pid}</span></button>{:else}{a.name} (pid {a.pid}){/if}{/snippet}

{#if parent || children.length}
  <p class="relations">
    {#if parent}<span>Started from {@render who(parent)}</span>{/if}
    {#if children.length}<span>Started {#each children as child, i}{#if i}, {/if}{@render who(child)}{/each}</span>{/if}
  </p>
{/if}

<style>
  .relations { margin: 0; display: grid; gap: 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  button { padding: 0; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  button:hover { color: var(--text); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
</style>
