<script lang="ts">
  // Header stats: open findings by severity, last scan, and coverage.
  import { SEVERITY_ORDER, ago, type Severity } from "../../lib/analyzers";
  import { cap, type SecuritySummary } from "../../lib/security-view";
  let { summary, active = [], ontoggle }: { summary: SecuritySummary; active?: Severity[]; ontoggle?: (s: Severity) => void } = $props();
  import StatStrip from "../ui/StatStrip.svelte";
  const stats = $derived([
    { id: "last", label: "Last scan", value: summary.lastRun ? ago(summary.lastRun) : "Never", meta: `${summary.analyzersOn} ${summary.analyzersOn === 1 ? "analyzer" : "analyzers"} on` },
    { id: "agents", label: "Agents scanned", value: summary.agents.scanned, of: summary.agents.total, bar: true },
    { id: "skills", label: "Skills scanned", value: summary.skills.scanned, of: summary.skills.total, bar: true },
  ]);
</script>

<StatStrip {stats} label="Security summary">
  {#snippet lead()}
    <span class="k">Open findings</span>
    <span class="row">
      <b class="big">{summary.open}</b>
      {#each SEVERITY_ORDER.filter((s) => s !== "info" || summary.counts.info) as s (s)}
        <button type="button" class="sev sev-{s}" class:on={active.includes(s)} class:zero={!summary.counts[s]} aria-pressed={active.includes(s)} onclick={() => ontoggle?.(s)}>
          <i aria-hidden="true"></i>{summary.counts[s]} <span>{cap(s)}</span>
        </button>
      {/each}
    </span>
  {/snippet}
</StatStrip>

<style>
  .k { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); }
  b { font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; color: var(--text); }
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
</style>
