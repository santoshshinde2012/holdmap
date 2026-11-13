<script lang="ts">
  // Process-tree memory for one agent. Observable counters stay in the summary.
  // A counter the OS did not return stays in the platform-limits disclosure, never as zero.
  import ExportActions from "../ui/ExportActions.svelte";
  import { memoryFor, memorySummary, processGroups, processNoun, readingBits, sharePercent, shareText, splitCounters, trendView } from "../../lib/agent-memory";
  import { humanBytes } from "../../lib/format";
  import type { Agent, AgentsReport } from "../../lib/types";

  let { agent, report }: { agent: Agent; report: AgentsReport } = $props();
  const memory = $derived(memoryFor(report, agent.id));
  const counters = $derived(memory ? splitCounters(memory) : { known: [], unknown: [] });
  const groups = $derived(memory ? processGroups(memory) : []);
  const processCount = $derived(groups.reduce((sum, group) => sum + group.processes.length, 0));
  const share = $derived(memory ? sharePercent(memory.share_ppm) : null);
  const trend = $derived(memory ? trendView(memory.trend) : null);
  const bar = $derived(share == null ? 0 : Math.max(0, Math.min(100, share)));
</script>

<section class="mem" aria-labelledby="agent-memory-{agent.id}">
  <h4 id="agent-memory-{agent.id}">Memory</h4>
  <div class="body">
    {#if !memory}
      <p>Platform counters were not in this report. The process-table total is {humanBytes(agent.memory_bytes)}.</p>
    {:else}
      <p class="headline"><b>{humanBytes(memory.headline_bytes)}</b> <span>{memory.headline_meaning}</span></p>
      <div class="share">
        {#if share != null}
          <div class="meter" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(share * 10) / 10} aria-label="Share of machine RAM">
            <span style="width: {bar}%"></span>
          </div>
          <p>{share.toFixed(1)}% of machine RAM</p>
        {:else}
          <p>Share of machine RAM: {shareText(memory.share_ppm)}</p>
        {/if}
        {#if trend}
          <p class="trend trend-{memory.trend.direction}">
            <span class="badge">{trend.label}{trend.delta ? ` ${trend.delta}` : ""}</span>
            {#if trend.why}<span class="why">{trend.why}</span>{/if}
          </p>
        {/if}
      </div>
      {#if counters.known.length}
        <dl>
          {#each counters.known as row (row.label)}
            <div>
              <dt>{row.label}</dt>
              <dd>{row.text}</dd>
            </div>
          {/each}
        </dl>
      {:else}
        <p class="why">No platform counter in this report was observable. The limits below say why.</p>
      {/if}
      {#if counters.unknown.length || memory.limits.length}
        <details class="limits">
          <summary>Platform limits &amp; unavailable metrics <span>{counters.unknown.length}</span></summary>
          {#if counters.unknown.length}
            <dl>
              {#each counters.unknown as row (row.label)}
                <div>
                  <dt>{row.label}</dt>
                  <dd class="unknown">{row.text}</dd>
                </div>
              {/each}
            </dl>
          {/if}
          {#each memory.limits as limit, i (i)}
            <p class="why">{limit}</p>
          {/each}
        </details>
      {/if}
      <details class="tree" open>
        <summary>Processes <span>{processNoun(processCount)}</span></summary>
        {#if groups.length}
          {#each groups as group (group.role)}
            <div class="role">
              <p class="role-name">{group.label} <span>{processNoun(group.processes.length)}</span> <b>{humanBytes(group.processes.reduce((sum, row) => sum + row.scan_bytes, 0))}</b></p>
              <ul>
                {#each group.processes as row (row.pid)}
                  <li class:large={row.dominant}>
                    <span class="who">{row.name} <span class="mono">pid {row.pid}</span></span>
                    <span class="fig">{humanBytes(row.scan_bytes)}{#if readingBits(row.reading)} · {readingBits(row.reading)}{/if}</span>
                    {#if row.dominant && row.dominant_reason}<span class="why">{row.dominant_reason}</span>{/if}
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        {:else}
          <p class="why">No per-process counters were listed in this report.</p>
        {/if}
      </details>
      <ExportActions label="Export memory" copy={() => memorySummary(agent, memory)} files={[{ label: "JSON", filename: `${agent.product}-memory.json`, build: () => memory }]} />
    {/if}
  </div>
</section>

<style>
  .mem { display: grid; gap: 6px; min-width: 0; }
  h4 { margin: 8px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--muted); }
  .body { display: grid; gap: 8px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .headline { margin: 0; color: var(--text); }
  .headline b { font-size: var(--fs-body); font-variant-numeric: tabular-nums; }
  .headline span, p { margin: 0; }
  .share { display: grid; gap: 4px; }
  .meter { height: 8px; border-radius: var(--r-full); background: var(--surface-2); overflow: hidden; }
  .meter span { display: block; height: 100%; border-radius: inherit; background: var(--accent); }
  .trend { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .badge { display: inline-flex; align-items: center; padding: 0 8px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .trend-rising .badge { background: var(--warn-soft); color: var(--warn); }
  .trend-falling .badge { background: var(--ok-soft); color: var(--ok); }
  dl { display: grid; grid-template-columns: repeat(auto-fill, minmax(128px, 1fr)); gap: 6px 10px; margin: 0; }
  div { min-width: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  dd.unknown { color: var(--muted); font-variant-numeric: normal; }
  details { min-width: 0; }
  summary { cursor: pointer; color: var(--text-2); font-weight: var(--fw-medium); }
  summary span { color: var(--muted); font-weight: var(--fw-regular); }
  summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 4px; }
  .limits, .tree { display: grid; gap: 6px; }
  .role { display: grid; gap: 3px; }
  .role-name { display: flex; flex-wrap: wrap; gap: 6px; align-items: baseline; margin: 4px 0 0; color: var(--text); font-weight: var(--fw-semibold); }
  .role-name b { font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  li { min-width: 0; display: grid; gap: 1px; padding: 4px 6px; border-radius: 8px; }
  li.large { background: var(--warn-soft); }
  .who { color: var(--text); overflow-wrap: anywhere; }
  .fig, .why { color: var(--muted); overflow-wrap: anywhere; }
</style>
