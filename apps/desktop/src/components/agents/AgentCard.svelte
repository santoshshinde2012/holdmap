<script lang="ts">
  // One agent in the rail: monogram, access headline, footprint counts and resources. Selecting
  // it opens the full picture in the Agents detail panel; the card itself stays compact.
  import { humanBytes, humanDuration } from "../../lib/format";
  import { KIND_LABEL, accessHeadline, counts, countsLine, monogram, startedFrom } from "../../lib/agents";
  import type { Agent, AgentsReport } from "../../lib/types";

  let {
    agent,
    report,
    selected,
    color,
    onselect,
    onnavigate,
    now = Date.now(),
  }: {
    agent: Agent;
    report: AgentsReport;
    selected: boolean;
    /** The agent's colour in the graph. */
    color: string;
    onselect: () => void;
    onnavigate?: (id: string) => void;
    now?: number;
  } = $props();

  let el = $state<HTMLElement | undefined>();
  // Selecting from the map or with ↑/↓ may land on a card scrolled out of the rail.
  $effect(() => { if (selected) el?.scrollIntoView?.({ block: "nearest" }); });
  const head = $derived(accessHeadline(agent));
  const c = $derived(counts(agent));
  const parent = $derived(report.agents.find((a) => a.id === agent.parent) ?? null);
  const childAgents = $derived(startedFrom(report, agent.id));
  const up = $derived(agent.started_at ? humanDuration(now / 1000 - agent.started_at) : null);
  // Segments wrap as whole units, so a narrow rail never cuts a PID or count in half.
  const meta = $derived([KIND_LABEL[agent.kind], agent.vendor !== "unknown" ? agent.vendor : null, `pid ${agent.pid}`, up ? `up ${up}` : null].filter((part): part is string => !!part));
  const nums = $derived([...countsLine(c).split(" · "), `${c.processes} ${c.processes === 1 ? "process" : "processes"}`, `${c.tools} ${c.tools === 1 ? "tool" : "tools"}`]);
</script>

{#snippet segments(parts: string[])}{#each parts as part, i}{#if i}{" "}{/if}<span class="seg">{i ? "· " : ""}{part}</span>{/each}{/snippet}


<article class="card" bind:this={el} class:on={selected} style="--agent: {color}" aria-label={agent.name}>
  <button class="head" aria-pressed={selected} aria-controls={selected ? "agent-details" : undefined} onclick={onselect}>
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <span class="who">
      <span class="name">{agent.name}</span>
      <span class="meta">{@render segments(meta)}</span>
    </span>
    <span class="acc lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.text}</span>
    <span class="nums">{@render segments(nums)}</span>
    <span class="resources mono">{agent.cpu_percent.toFixed(agent.cpu_percent < 10 ? 1 : 0)}% CPU · {humanBytes(agent.memory_bytes)} total memory</span>
  </button>

  {#if parent || childAgents.length}
    <div class="relations">
      {#if parent}<span>Started from {#if onnavigate}<button onclick={() => onnavigate?.(parent.id)}>{parent.name} <span class="mono">{parent.pid}</span></button>{:else}{parent.name} (pid {parent.pid}){/if}</span>{/if}
      {#if childAgents.length}<span>Started {#each childAgents as child, i}{#if i}, {/if}{#if onnavigate}<button onclick={() => onnavigate?.(child.id)}>{child.name} <span class="mono">{child.pid}</span></button>{:else}{child.name} (pid {child.pid}){/if}{/each}</span>{/if}
    </div>
  {/if}

</article>

<style>
  .card { border: 1px solid var(--border); border-radius: 14px; background: var(--surface); overflow: hidden; }
  .card.on { border-color: var(--agent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 16%, transparent); }
  .head:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: 14px; }
  .head { width: 100%; display: grid; grid-template-columns: auto 1fr; grid-template-areas: "tile who" "tile acc" "nums nums" "resources resources"; gap: 2px 10px; padding: 10px 12px; border: 0; background: transparent; text-align: left; cursor: pointer; color: var(--text); }
  .tile { grid-area: tile; width: 36px; height: 36px; margin-top: 2px; display: grid; place-items: center; border-radius: 10px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); letter-spacing: var(--ls-body); }
  .who { grid-area: who; min-width: 0; display: grid; }
  .name { font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: flex; flex-wrap: wrap; column-gap: 0.3em; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .seg { white-space: nowrap; }
  .acc { grid-area: acc; display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .acc i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); flex: none; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .acc.lv-elevated { color: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .nums { grid-area: nums; display: flex; flex-wrap: wrap; column-gap: 0.3em; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .resources { grid-area: resources; font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--text-2); }
  .relations { padding: 0 12px 8px; display: grid; gap: 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .relations button { padding: 0; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
</style>
