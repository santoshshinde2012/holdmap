<script lang="ts">
  // Findings: toolbar, grouped list and empty states. State lives in the view-model.
  import * as api from "../../lib/api";
  import FindingsToolbar from "./FindingsToolbar.svelte";
  import FindingsList from "./FindingsList.svelte";
  import ResultsTable from "./ResultsTable.svelte";
  import { redactHome } from "../../lib/report-export";
  import SecurityEmpty from "./SecurityEmpty.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { ago } from "../../lib/analyzers";
  import { applyFilter, changeSummary, emptyFilter, findingItems, groupFindings, sortFindings, type FindingFilter, type GroupBy, type SortSpec } from "../../lib/security-view";
  import type { FindingState } from "../../lib/analyzers";

  let { agents, filter = $bindable(emptyFilter()), onsetup, view = "list" }: { agents: { product: string; name: string }[]; filter?: FindingFilter; onsetup?: () => void; view?: "list" | "table" } = $props();
  let sort = $state<SortSpec>({ key: "severity", dir: "desc" });
  let groupBy = $state<GroupBy>("severity");
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  const names = $derived(new Map(agents.map((a) => [a.product, a.name])));
  const all = $derived(findingItems(insightsStore.insights, insightsStore.analyzers, names));
  const shown = $derived(applyFilter(all, filter));
  const groups = $derived(groupFindings(view === "table" ? sortFindings(shown, sort) : shown, groupBy));
  const anyOn = $derived(insightsStore.analyzers.some((a) => a.state === "approved"));
  const filtered = $derived(!!(filter.query || filter.agent || filter.severities.length || filter.newOnly));
  const changes = $derived(changeSummary(insightsStore.insights, insightsStore.analyzers));

  async function review(fingerprint: string, state: FindingState, reason?: string) {
    busy = fingerprint; error = null;
    try { await api.reviewFinding(fingerprint, state, state === "snoozed" ? 7 : undefined, reason); await insightsStore.refresh(); }
    catch (e) { error = String(e); }
    finally { busy = null; }
  }
  function rescan(id: string) {
    const a = insightsStore.analyzers.find((x) => x.id === id);
    if (a) void securityRuns.start([a]);
  }
</script>

{#if !anyOn && !all.length}
  <SecurityEmpty kind="setup" onprimary={onsetup} />
{:else}
  <div class="panel">
    {#if changes.compared}
      <div class="changes" data-run-changes role="status">
        <span><b>{changes.new}</b> new · <b>{changes.fixed}</b> fixed since the previous run{#if changes.sinceMs !== null} <span class="since" title={new Date(changes.sinceMs).toLocaleString()}>({ago(changes.sinceMs)})</span>{/if}</span>
        {#if changes.new}<button type="button" class="link" onclick={() => (filter = { ...filter, newOnly: true })}>Show new only</button>{/if}
        {#if changes.fixedTop.length}
          <details><summary>What was fixed</summary><ul>{#each changes.fixedTop as f, i (i)}<li><span class="sev-pill {f.severity}">{f.severity}</span> {f.title} <small>{f.analyzer}</small></li>{/each}</ul>{#if changes.fixed > changes.fixedTop.length}<small>and {changes.fixed - changes.fixedTop.length} more</small>{/if}</details>
        {/if}
      </div>
    {/if}
    <FindingsToolbar bind:filter bind:groupBy {agents} shown={shown.length} />
    {#if groups.length}
      {#if view === "table"}
        <div class="tables">
          {#each groups as g (g.key)}
            {@const label = groupBy === "folder" ? redactHome(g.label) : g.label}
            <section aria-label="{label} findings"><h4>{label} <span>{g.items.length}</span></h4><ResultsTable items={g.items} bind:sort /></section>
          {/each}
        </div>
      {:else}
        <FindingsList {groups} {groupBy} {busy} onreview={review} onrescan={securityRuns.busy ? undefined : rescan} />
      {/if}
    {:else}
      <SecurityEmpty kind={filtered ? "nomatch" : "clear"} onclear={() => (filter = { ...emptyFilter(), showClosed: filter.showClosed })} />
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
{/if}

<style>
  .panel { display: grid; align-content: start; gap: 12px; }
  .tables { display: grid; gap: 14px; }
  .tables section { display: grid; gap: 6px; min-width: 0; }
  h4 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  h4 span { color: var(--muted); font-weight: var(--fw-regular); margin-left: 4px; font-variant-numeric: tabular-nums; }
  .changes { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface-2); font-size: var(--fs-body-sm); color: var(--text-2); }
  .changes b { color: var(--text); font-variant-numeric: tabular-nums; }
  .since, .changes small { color: var(--muted); font-size: var(--fs-caption); }
  .link { border: 0; background: none; padding: 0; color: var(--accent); text-decoration: underline; font: inherit; cursor: pointer; }
  .link:focus-visible, .changes summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .changes details { flex-basis: 100%; }
  .changes summary { cursor: pointer; color: var(--text); }
  .changes ul { margin: 6px 0 0; padding: 0; list-style: none; display: grid; gap: 4px; }
  .err { margin: 0; color: var(--danger); font-size: var(--fs-caption); }
</style>
