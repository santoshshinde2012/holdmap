<script lang="ts">
  // Process-tree memory telemetry for one agent: headline footprint, role composition bar,
  // retained trend area chart, observable counters, and proportional per-process rows.
  import Icon from "../Icon.svelte";
  import { agentDomId } from "../../lib/agent-details";
  import ExportActions from "../ui/ExportActions.svelte";
  import { headlineLabel, memoryFor, memorySummary, processGroups, processNoun, readingBits, sharePercent, shareText, splitCounters, trendSparkline, trendView } from "../../lib/agent-memory";
  import { humanBytes } from "../../lib/format";
  import type { Agent, AgentsReport } from "../../lib/types";

  const roleColor = (r: string) =>
    r === "agent" ? "var(--accent)" : r === "mcp_server" ? "var(--tone-violet)" : r === "language_server" ? "var(--tone-blue)" : r === "helper" ? "var(--tone-amber)" : "var(--muted)";

  let { agent, report }: { agent: Agent; report: AgentsReport } = $props();
  let roleFilter = $state<string | null>(null);

  const memory = $derived(memoryFor(report, agent.id));
  const counters = $derived(memory ? splitCounters(memory) : { known: [], unknown: [] });
  const groups = $derived(memory ? processGroups(memory) : []);
  const processCount = $derived(groups.reduce((sum, group) => sum + group.processes.length, 0));
  const share = $derived(memory ? sharePercent(memory.share_ppm) : null);
  const trend = $derived(memory ? trendView(memory.trend) : null);
  const spark = $derived(memory ? trendSparkline(memory.trend.samples, 260, 48) : null);
  const bar = $derived(share == null ? 0 : Math.max(0, Math.min(100, share)));

  const groupTotals = $derived.by(() => {
    const sums = groups.map((g) => ({ role: g.role, label: g.label, bytes: g.processes.reduce((n, p) => n + p.scan_bytes, 0) }));
    const total = sums.reduce((n, g) => n + g.bytes, 0);
    return sums.map((g) => ({ ...g, exactPct: total ? Math.round((g.bytes / total) * 100) : 0, color: roleColor(g.role) }));
  });

  const maxProcBytes = $derived(
    memory?.processes.length ? Math.max(1, ...memory.processes.map((p) => p.scan_bytes)) : 1,
  );
  const visibleGroups = $derived(
    roleFilter && groups.some((g) => g.role === roleFilter)
      ? groups.filter((g) => g.role === roleFilter)
      : groups,
  );
</script>

<section class="mem" id="section-memory-{agentDomId(agent.id)}" aria-labelledby="agent-memory-{agentDomId(agent.id)}">
  <div class="sec-head">
    <h4 id="agent-memory-{agentDomId(agent.id)}"><Icon name="activity" size={13} />RAM</h4>
    {#if trend}
      <span class="trend-badge dir-{memory?.trend.direction ?? 'unknown'}" title={trend.rule}>
        <i aria-hidden="true"></i>{trend.label}{#if trend.delta} · {trend.delta}{/if}
      </span>
    {/if}
  </div>

  <div class="body">
    {#if !memory}
      <div class="card">
        <p class="fallback">Platform counters were not in this report. The process-table total is <b>{humanBytes(agent.memory_bytes)}</b>.</p>
      </div>
    {:else}
      <div class="card hero">
        <div class="hero-top">
          <b class="headline-val">{humanBytes(memory.headline_bytes)}</b>
          {#if share != null}<span class="share-pill">{share.toFixed(1)}% of machine RAM</span>{/if}
        </div>
        <span class="counter-row">
          <span class="counter-pill" class:mixed={!!memory.headline_mixed}>Counter: {headlineLabel(memory)}</span>
        </span>
        <span class="meaning">{memory.headline_meaning}</span>
        {#if memory.headline_mixed}<span class="meaning mixed-why">{memory.headline_mixed}</span>{/if}

        {#if share != null}
          <div class="meter-wrap">
            <div class="meter" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(share * 10) / 10} aria-label="Share of machine RAM">
              <span class="fill" style="width: {bar}%"></span>
              <i class="tick" style="left: 25%" aria-hidden="true"></i>
              <i class="tick" style="left: 50%" aria-hidden="true"></i>
              <i class="tick" style="left: 75%" aria-hidden="true"></i>
            </div>
          </div>
        {:else}
          <p class="why">Share of machine RAM: {shareText(memory.share_ppm)}</p>
        {/if}

        {#if groupTotals.length > 1}
          <div class="comp" aria-label="Memory by process role">
            <div class="comp-head">
              <span>By role</span>
              {#if roleFilter}<button type="button" class="clear-role" onclick={() => (roleFilter = null)}>Show all</button>{/if}
            </div>
            <div class="comp-bar" aria-hidden="true">
              {#each groupTotals as seg (seg.role)}
                <span class="seg" style="flex-grow: {seg.bytes || 1}; background: {seg.color}" title="{seg.label}: {humanBytes(seg.bytes)} ({seg.exactPct}%)"></span>
              {/each}
            </div>
            <div class="comp-legend">
              {#each groupTotals as seg (seg.role)}
                <button
                  type="button"
                  class="role-chip"
                  class:active={roleFilter === seg.role}
                  aria-pressed={roleFilter === seg.role} onclick={() => (roleFilter = roleFilter === seg.role ? null : seg.role)}
                >
                  <i style="background: {seg.color}" aria-hidden="true"></i>
                  <span>{seg.label}</span>
                  <b>{humanBytes(seg.bytes)}</b>
                  <em>{seg.exactPct}%</em>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        {#if trend}
          <div class="trend-well dir-{memory.trend.direction}" title={trend.rule}>
            {#if spark}
              <svg class="spark" viewBox="0 0 260 48" preserveAspectRatio="none" aria-hidden="true">
                <line class="guide" x1="0" y1="12" x2="260" y2="12" />
                <line class="guide" x1="0" y1="36" x2="260" y2="36" />
                <polygon fill="currentColor" fill-opacity="0.14" points={spark.areaPoints} />
                <polyline fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" points={spark.points} />
                <circle class="pulse" cx={spark.lastX} cy={spark.lastY} r="4.5" fill="currentColor" fill-opacity="0.2" />
                <circle cx={spark.lastX} cy={spark.lastY} r="2.4" fill="currentColor" />
              </svg>
              <div class="trend-foot">
                <span>Range {humanBytes(spark.minBytes)} – {humanBytes(spark.maxBytes)}</span>
                <span>{trend.sampleCount} samples{trend.span ? ` · ${trend.span}` : ""}</span>
              </div>
            {:else}
              <div class="trend-foot single">
                <span>{trend.sampleCount} {trend.sampleCount === 1 ? "sample" : "samples"}{trend.span ? ` · ${trend.span}` : ""}</span>
                {#if trend.why}<span>{trend.why}</span>{/if}
              </div>
            {/if}
          </div>
        {/if}
      </div>

      {#if counters.known.length}
        <dl class="counters">
          {#each counters.known as row (row.label)}
            <div class="counter-cell">
              <dt>{row.label}</dt>
              <dd>{row.text}</dd>
            </div>
          {/each}
        </dl>
      {:else}
        <p class="why">No platform counter in this report was observable. The limits below say why.</p>
      {/if}

      <details class="tree" open>
        <summary>
          <span class="sum-left">Processes <span class="count-pill">{processNoun(processCount)}</span></span>
          {#if roleFilter}<span class="filter-tag">{groups.find((g) => g.role === roleFilter)?.label}</span>{/if}
        </summary>
        {#if visibleGroups.length}
          <div class="groups">
            {#each visibleGroups as group (group.role)}
              <div class="role">
                <p class="role-name">
                  <span class="role-title"><i class="dot" style="background: {roleColor(group.role)}" aria-hidden="true"></i>{group.label} <span class="role-count">{processNoun(group.processes.length)}</span></span>
                  <b>{humanBytes(group.processes.reduce((sum, row) => sum + row.scan_bytes, 0))}</b>
                </p>
                <ul>
                  {#each group.processes as row (row.pid)}
                    {@const bits = readingBits(row.reading)}
                    {@const pct = Math.max(4, Math.round((row.scan_bytes / maxProcBytes) * 100))}
                    <li class:large={row.dominant}>
                      <span class="proc-bar" style="width: {pct}%; background: {row.dominant ? 'var(--warn)' : (roleColor(group.role))}" aria-hidden="true"></span>
                      <div class="who-row">
                        <span class="who">{row.name}</span>
                        <span class="badges">
                          {#if row.dominant}<span class="large-pill">Large</span>{/if}
                          <span class="pid mono">pid {row.pid}</span>
                        </span>
                      </div>
                      <span class="fig"><b class="scan">{humanBytes(row.scan_bytes)}</b>{#if bits}{" · "}{bits}{/if}</span>
                      {#if row.dominant && row.dominant_reason}<span class="why">{row.dominant_reason}</span>{/if}
                    </li>
                  {/each}
                </ul>
              </div>
            {/each}
          </div>
        {:else}
          <p class="why">No per-process counters were listed in this report.</p>
        {/if}
      </details>

      {#if counters.unknown.length || memory.limits.length}
        <details class="limits">
          <summary>Platform limits &amp; unavailable metrics <span class="count-pill">{counters.unknown.length}</span></summary>
          {#if counters.unknown.length}
            <dl class="limits-dl">
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

      <ExportActions label="Export memory" copy={() => memorySummary(agent, memory)} files={[{ label: "JSON", filename: `${agent.product}-memory.json`, build: () => memory }]} />
    {/if}
  </div>
</section>

<style>
  .mem { display: grid; gap: 8px; min-width: 0; }
  .sec-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 4px; }
  h4 { margin: 0; display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .trend-badge { display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .trend-badge i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .trend-badge.dir-rising { background: var(--warn-soft); color: var(--warn); }
  .trend-badge.dir-rising i { background: var(--warn); }
  .trend-badge.dir-falling { background: var(--ok-soft); color: var(--ok); }
  .trend-badge.dir-falling i { background: var(--ok); }
  .trend-badge.dir-flat i { background: var(--agent, var(--accent)); }

  .body { display: grid; gap: 8px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .card { display: grid; gap: 10px; padding: 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; box-shadow: var(--shadow-sm); }
  .fallback { margin: 0; color: var(--text-2); }
  .hero-top { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 10px; }
  .counter-row { display: flex; flex-wrap: wrap; gap: 4px; }
  .counter-pill { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 8px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font-weight: var(--fw-medium); }
  .counter-pill.mixed { background: var(--warn-soft); color: var(--warn); }
  .headline-val { font-size: var(--fs-title); line-height: var(--lh-title); letter-spacing: var(--ls-title); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  .meaning { color: var(--muted); overflow-wrap: anywhere; }
  .share-pill { padding: 2px 9px; border-radius: var(--r-full); background: color-mix(in srgb, var(--agent, var(--accent)) 12%, var(--surface)); color: var(--text); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; flex: none; }

  .meter-wrap { display: grid; gap: 4px; }
  .meter { position: relative; height: 8px; border-radius: var(--r-full); background: var(--surface-2); overflow: hidden; }
  .meter .fill { display: block; height: 100%; border-radius: inherit; background: linear-gradient(90deg, color-mix(in srgb, var(--agent, var(--accent)) 82%, white), var(--agent, var(--accent))); transition: width var(--dur-2) var(--ease); }
  .meter .tick { position: absolute; top: 0; bottom: 0; width: 1px; background: color-mix(in srgb, var(--bg) 55%, transparent); }

  .comp { display: grid; gap: 6px; padding-top: 4px; border-top: 1px solid var(--border); }
  .comp-head { display: flex; align-items: center; justify-content: space-between; color: var(--muted); font-weight: var(--fw-medium); }
  .clear-role { border: 0; padding: 0; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  .comp-bar { display: flex; gap: 2px; height: 7px; border-radius: var(--r-full); overflow: hidden; background: var(--surface-2); }
  .comp-bar .seg { min-width: 4px; border-radius: 2px; transition: opacity var(--dur-1) var(--ease); }
  .comp-legend { display: flex; flex-wrap: wrap; gap: 4px; }
  .role-chip { display: inline-flex; align-items: center; gap: 5px; padding: 2px 7px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font: inherit; cursor: pointer; font-variant-numeric: tabular-nums; }
  .role-chip:hover { border-color: var(--border-strong); color: var(--text); }
  .role-chip.active { border-color: var(--agent, var(--accent)); background: color-mix(in srgb, var(--agent, var(--accent)) 12%, var(--surface)); color: var(--text); }
  .role-chip i { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .role-chip b { color: var(--text); font-weight: var(--fw-semibold); }
  .role-chip em { font-style: normal; color: var(--muted); }

  .trend-well { display: grid; gap: 4px; padding: 8px 10px 6px; border-radius: var(--r-md); background: var(--surface-2); color: var(--agent, var(--accent)); }
  .trend-well.dir-rising { color: var(--warn); }
  .trend-well.dir-falling { color: var(--ok); }
  .spark { width: 100%; height: 42px; display: block; overflow: visible; }
  .guide { stroke: var(--border); stroke-dasharray: 2 3; stroke-width: 1; }
  .trend-foot { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 8px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .trend-foot.single { justify-content: flex-start; }

  .counters { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); overflow: hidden; margin: 0; }
  .counter-cell { padding: 8px 10px; min-width: 0; box-shadow: -1px 0 0 var(--border), 0 -1px 0 var(--border); }
  .limits-dl { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 6px 10px; margin: 0; padding: 8px; border-radius: var(--r-md); background: var(--surface-2); }
  div { min-width: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; font-weight: var(--fw-semibold); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  dd.unknown { color: var(--muted); font-variant-numeric: normal; font-weight: var(--fw-regular); font-size: var(--fs-caption); line-height: var(--lh-caption); }

  details { min-width: 0; }
  summary { cursor: pointer; color: var(--text-2); font-weight: var(--fw-semibold); display: flex; align-items: center; justify-content: space-between; gap: 6px; padding: 4px 2px; }
  .sum-left { display: inline-flex; align-items: center; gap: 6px; }
  .count-pill { padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .filter-tag { padding: 0 7px; border-radius: var(--r-full); background: var(--accent-soft); color: var(--accent); font-weight: var(--fw-medium); }
  summary:focus-visible, button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 4px; }

  .limits, .tree { display: grid; gap: 6px; }
  .groups { display: grid; gap: 8px; }
  .role { display: grid; gap: 4px; }
  .role-name { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 6px; align-items: baseline; margin: 0; color: var(--text); font-weight: var(--fw-semibold); }
  .role-title { display: inline-flex; align-items: center; gap: 6px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .role-count { color: var(--muted); font-weight: var(--fw-regular); }
  .role-name b { font-variant-numeric: tabular-nums; }

  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  li { position: relative; min-width: 0; display: grid; gap: 3px; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); overflow: hidden; }
  li.large { border-color: color-mix(in srgb, var(--warn) 42%, var(--border)); }
  .proc-bar { position: absolute; left: 0; bottom: 0; height: 2px; opacity: 0.55; border-radius: 1px; pointer-events: none; }
  .who-row { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 8px; min-width: 0; }
  .who { color: var(--text); font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .badges { display: inline-flex; align-items: center; gap: 4px; flex: none; }
  .large-pill { padding: 0 6px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-weight: var(--fw-semibold); }
  .pid { padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); flex: none; }
  .fig, .why { color: var(--muted); overflow-wrap: anywhere; margin: 0; }
  .scan { color: var(--text); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
</style>
