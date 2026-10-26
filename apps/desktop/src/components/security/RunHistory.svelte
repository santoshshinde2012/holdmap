<script lang="ts">
  // Past runs per analyzer: when, which version, how it ended and what it found.
  import * as api from "../../lib/api";
  import { SEVERITY_ORDER, ago, type AnalyzerView, type RunRecord } from "../../lib/analyzers";
  import { elapsed } from "../../lib/security-view";

  let { analyzers, version = 0 }: { analyzers: Pick<AnalyzerView, "id" | "name">[]; version?: number } = $props();
  let rows = $state<(RunRecord & { name: string })[]>([]);
  let loading = $state(true);
  $effect(() => {
    void version;
    const list = analyzers.map((a) => ({ id: a.id, name: a.name }));
    loading = true;
    Promise.all(list.map((a) => api.analyzerHistory(a.id).then((h) => h.map((r) => ({ ...r, name: a.name })), () => [])))
      .then((all) => { rows = all.flat().sort((a, b) => b.run_at_ms - a.run_at_ms).slice(0, 30); loading = false; });
  });
  const counts = (r: RunRecord) => SEVERITY_ORDER.filter((s) => r.counts[s]).map((s) => `${r.counts[s]} ${s}`).join(", ") || "no findings";
</script>

<section class="hist" aria-labelledby="run-history">
  <h3 id="run-history">Run history</h3>
  {#if loading}<p class="sub" role="status">Loading…</p>
  {:else if !rows.length}<p class="sub">No runs yet.</p>
  {:else}
    <table data-run-history>
      <thead><tr><th scope="col">When</th><th scope="col">Analyzer</th><th scope="col">Version</th><th scope="col">Result</th><th scope="col">Items</th><th scope="col">Findings</th><th scope="col">Changes</th><th scope="col">Took</th></tr></thead>
      <tbody>
        {#each rows as r (r.analyzer + r.run_at_ms)}
          <tr>
            <td title={new Date(r.run_at_ms).toLocaleString()}>{ago(r.run_at_ms)}</td>
            <td>{r.name}</td>
            <td>{r.analyzer_version ?? "unknown"}</td>
            <td class="st-{r.status}">{r.status}</td>
            <td>{r.subjects}</td>
            <td>{counts(r)}</td>
            <td>{r.new == null || r.fixed == null ? "—" : `${r.new} new · ${r.fixed} fixed`}</td>
            <td>{r.duration_ms == null ? "—" : elapsed(r.duration_ms)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .hist { display: grid; gap: 8px; }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body-sm); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  th { text-align: left; padding: 6px 10px; background: var(--surface-2); color: var(--text-2); font-weight: var(--fw-semibold); }
  td { padding: 6px 10px; border-top: 1px solid var(--border); font-variant-numeric: tabular-nums; }
  .st-failed { color: var(--danger); } .st-partial { color: var(--tone-amber); }
</style>
