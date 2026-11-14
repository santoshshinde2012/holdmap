<script lang="ts">
  // One agent in the rail: identity header, footprint KPI chips and resource telemetry. Selecting
  // it opens the full picture in the Agents detail panel; the card itself stays compact.
  import { agentMemoryText, memoryFor } from "../../lib/memory-text";
  import { ACCESS_LABEL, accessHeadline, agentMeta, countChips, counts, monogram } from "../../lib/agents";
  import AgentRelations from "./AgentRelations.svelte";
  import { cpuLabel } from "../../lib/format";
  import type { Agent, AgentsReport } from "../../lib/types";

  let {
    agent,
    report,
    selected,
    color,
    maxMemory,
    onselect,
    onnavigate,
    onhover,
    now = Date.now(),
  }: {
    agent: Agent;
    report: AgentsReport;
    selected: boolean;
    /** The agent's colour in the graph. */
    color: string;
    maxMemory?: number;
    onselect: () => void;
    onnavigate?: (id: string) => void;
    /** Pointer over / off the card, so the map can light up this agent's footprint. */
    onhover?: (on: boolean) => void;
    now?: number;
  } = $props();

  let el = $state<HTMLElement | undefined>();
  // Selecting from the map or with ↑/↓ may land on a card scrolled out of the rail.
  $effect(() => { if (selected) el?.scrollIntoView?.({ block: "nearest" }); });
  const head = $derived(accessHeadline(agent));
  const c = $derived(counts(agent));
  const peakMem = $derived(maxMemory && maxMemory > 0 ? maxMemory : Math.max(1, ...report.agents.map((a) => a.memory_bytes)));
  const memPct = $derived(Math.max(3, Math.min(100, Math.round((agent.memory_bytes / peakMem) * 100))));
</script>

<article class="card" bind:this={el} class:on={selected} style="--agent: {color}" aria-label={agent.name} onpointerenter={() => onhover?.(true)} onpointerleave={() => onhover?.(false)}>
  <button class="head" aria-pressed={selected} aria-controls={selected ? "agent-details" : undefined} onclick={onselect}>
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <span class="who">
      <span class="top"><span class="name">{agent.name}</span> <span class="acc lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.level === "elevated" ? head.text : ACCESS_LABEL[head.level]}</span></span>
      <span class="meta">{agentMeta(agent, now, false)}</span>
    </span>
    <span class="kpis">{#each countChips(c, true) as [n, label]}<span class="kpi"><b>{n}</b> {label}</span> {/each}{#if c.stoppable}<span class="kpi warn"><b>{c.stoppable}</b> stoppable</span>{/if}</span>
    <span class="res">
      <span class="mem"><b>{agentMemoryText(agent, memoryFor(report, agent.id))}</b> <span class="res-sub">RAM</span></span>
      <span class="cpu" title={agent.cpu_unknown ?? undefined}>{cpuLabel(agent)}</span>
      <span class="mem-track" aria-hidden="true"><span class="mem-fill" style="width: {memPct}%"></span></span>
    </span>
  </button>

  <AgentRelations {agent} {report} {onnavigate} />
</article>

<style>
  .card { position: relative; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); overflow: hidden; transition: border-color var(--dur-1) var(--ease), box-shadow var(--dur-1) var(--ease), transform var(--dur-1) var(--ease); }
  .card::before { content: ""; position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 0 3px 3px 0; background: var(--agent); opacity: 0; transition: opacity var(--dur-1) var(--ease); }
  .card:hover { border-color: var(--border-strong); box-shadow: var(--shadow-sm); transform: translateY(-1px); }
  .card.on { border-color: color-mix(in srgb, var(--agent) 60%, var(--border)); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 14%, transparent); }
  .card.on::before { opacity: 1; }
  .head:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: 14px; }
  .head { width: 100%; display: grid; grid-template-columns: auto 1fr; grid-template-areas: "tile who" "kpis kpis" "res res"; gap: 8px 10px; padding: 11px 12px 10px 13px; border: 0; background: transparent; text-align: left; cursor: pointer; color: var(--text); font: inherit; }
  .tile { grid-area: tile; width: 36px; height: 36px; display: grid; place-items: center; border-radius: 10px; border: 1px solid color-mix(in srgb, var(--agent) 30%, transparent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); color: var(--agent); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); letter-spacing: var(--ls-body); }
  .who { grid-area: who; min-width: 0; display: grid; align-content: center; gap: 1px; }
  .top { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .name { min-width: 0; font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acc { flex: none; max-width: 60%; display: inline-flex; align-items: center; gap: 4px; margin-left: auto; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acc i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); flex: none; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .acc.lv-elevated { color: var(--danger); background: var(--danger-soft); border-color: color-mix(in srgb, var(--danger) 30%, transparent); }
  .kpis { grid-area: kpis; display: flex; flex-wrap: wrap; gap: 4px; }
  .kpi { display: inline-flex; align-items: baseline; gap: 3px; padding: 1px 7px; border-radius: var(--r-full); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .kpi b { color: var(--text); font-weight: var(--fw-semibold); }
  .kpi.warn { background: var(--warn-soft); color: var(--warn); }
  .kpi.warn b { color: var(--warn); }
  .res { grid-area: res; display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 5px 8px; }
  .mem { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .mem b { color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); }
  .cpu { padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r-full); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .mem-track { grid-column: 1 / -1; height: 4px; border-radius: var(--r-full); background: var(--surface-2); overflow: hidden; }
  .mem-fill { display: block; height: 100%; border-radius: inherit; background: var(--agent); opacity: 0.8; transition: width var(--dur-2) var(--ease); }
  .card > :global(.relations) { padding: 0 12px 9px 13px; }
  @media (prefers-reduced-motion: reduce) { .card, .card:hover { transition: none; transform: none; } }
</style>
