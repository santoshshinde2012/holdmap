<script lang="ts">
  // Process-tree memory for one agent. Every counter is the OS figure or "unknown" plus
  // the reason. Nothing here turns a missing counter into zero.
  import ExportActions from "../ui/ExportActions.svelte";
  import { counterRows, memoryFor, memorySummary, roleLabel, shareText } from "../../lib/agent-memory";
  import { humanBytes } from "../../lib/format";
  import type { Agent, AgentsReport } from "../../lib/types";

  let { agent, report }: { agent: Agent; report: AgentsReport } = $props();
  const memory = $derived(memoryFor(report, agent.id));
  const counters = $derived(memory ? counterRows(memory) : []);
</script>

<section class="mem" aria-labelledby="agent-memory-{agent.id}">
  <h4 id="agent-memory-{agent.id}">Memory</h4>
  <div class="body">
    {#if !memory}
      <p>Platform counters were not in this report. The process-table total is {humanBytes(agent.memory_bytes)}.</p>
    {:else}
      <p class="headline"><b>{humanBytes(memory.headline_bytes)}</b> <span>{memory.headline_meaning}</span></p>
      <p>Share of machine RAM: {shareText(memory.share_ppm)}</p>
      <p>Trend: {memory.trend.direction}{#if memory.trend.unknown} <span class="why">({memory.trend.unknown})</span>{/if}</p>
      <dl>
        {#each counters as row (row.label)}
          <div>
            <dt>{row.label}</dt>
            <dd class:unknown={!row.known}>{row.text}</dd>
          </div>
        {/each}
      </dl>
      {#if memory.groups.length}
        <ul aria-label="Memory by role">
          {#each memory.groups as group (group.role)}
            <li><span>{roleLabel(group.role)}</span> <b>{humanBytes(group.scan_bytes)}</b> <span>{group.processes}</span></li>
          {/each}
        </ul>
      {/if}
      {#if memory.processes.some((row) => row.dominant)}
        <ul aria-label="Large processes">
          {#each memory.processes.filter((row) => row.dominant) as row (row.pid)}
            <li>{row.name} <span class="mono">pid {row.pid}</span> · {humanBytes(row.scan_bytes)} · {row.role.replaceAll("_", " ")}. {row.dominant_reason}</li>
          {/each}
        </ul>
      {/if}
      {#each memory.limits as limit, i (i)}
        <p class="why">{limit}</p>
      {/each}
      <ExportActions label="Export memory" copy={() => memorySummary(agent, memory)} files={[{ label: "JSON", filename: `${agent.product}-memory.json`, build: () => memory }]} />
    {/if}
  </div>
</section>

<style>
  .mem { display: grid; gap: 6px; min-width: 0; }
  h4 { margin: 8px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--muted); }
  .body { display: grid; gap: 6px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .headline { margin: 0; color: var(--text); }
  .headline b { font-size: var(--fs-body); font-variant-numeric: tabular-nums; }
  p { margin: 0; }
  dl { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 4px 10px; margin: 0; }
  div { min-width: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
  dd.unknown { color: var(--muted); font-variant-numeric: normal; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 3px; }
  li { min-width: 0; }
  .why { color: var(--muted); }
</style>
