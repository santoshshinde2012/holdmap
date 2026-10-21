<script lang="ts">
  // Header stats: open findings by severity, last scan, and coverage.
  import { SEVERITY_ORDER, ago, type Severity } from "../../lib/analyzers";
  import { cap, type SecuritySummary } from "../../lib/security-view";
  let { summary, active = [], ontoggle }: { summary: SecuritySummary; active?: Severity[]; ontoggle?: (s: Severity) => void } = $props();
  const pct = (a: { scanned: number; total: number }) => (a.total ? Math.round((a.scanned / a.total) * 100) : 0);
</script>

<div class="summary" role="group" aria-label="Security summary">
  <div class="stat sevs">
    <span class="k">Open findings</span>
    <span class="row">
      <b class="big">{summary.open}</b>
      {#each SEVERITY_ORDER.filter((s) => s !== "info" || summary.counts.info) as s (s)}
        <button type="button" class="sev sev-{s}" class:on={active.includes(s)} class:zero={!summary.counts[s]} aria-pressed={active.includes(s)} onclick={() => ontoggle?.(s)}>
          <i aria-hidden="true"></i>{summary.counts[s]} <span>{cap(s)}</span>
        </button>
      {/each}
    </span>
  </div>
  <div class="stat"><span class="k">Last scan</span><b>{summary.lastRun ? ago(summary.lastRun) : "Never"}</b><span class="m">{summary.analyzersOn} {summary.analyzersOn === 1 ? "analyzer" : "analyzers"} on</span></div>
  <div class="stat"><span class="k">Agents scanned</span><b>{summary.agents.scanned}<small>/{summary.agents.total}</small></b><span class="bar" aria-hidden="true"><i style="width:{pct(summary.agents)}%"></i></span></div>
  <div class="stat"><span class="k">Skills scanned</span><b>{summary.skills.scanned}<small>/{summary.skills.total}</small></b><span class="bar" aria-hidden="true"><i style="width:{pct(summary.skills)}%"></i></span></div>
</div>

<style>
  .summary { display: grid; grid-template-columns: minmax(0, 2fr) repeat(3, minmax(0, 1fr)); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .stat { display: grid; align-content: start; gap: 4px; padding: 12px 16px; min-width: 0; }
  .stat + .stat { border-left: 1px solid var(--border); }
  .k { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); }
  b { font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; color: var(--text); }
  b small { font-size: var(--fs-body-sm); color: var(--muted); font-weight: var(--fw-medium); }
  .m { color: var(--muted); font-size: var(--fs-caption); }
  .row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .big { margin-right: 6px; }
  .sev { display: inline-flex; align-items: center; gap: 5px; white-space: pre; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; cursor: pointer; }
  .sev span { color: var(--muted); font-weight: var(--fw-medium); }
  .sev i { width: 7px; height: 7px; border-radius: 50%; background: var(--tone-gray); }
  .sev-critical i, .sev-high i { background: var(--danger); }
  .sev-medium i { background: var(--warn); }
  .sev-low i { background: var(--tone-blue); }
  .sev.zero { color: var(--muted); }
  .sev.zero i { opacity: 0.35; }
  .sev.on { border-color: var(--accent); background: var(--accent-soft); }
  .sev:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .bar { height: 4px; border-radius: 2px; background: var(--surface-3); overflow: hidden; margin-top: 2px; }
  .bar i { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
</style>
