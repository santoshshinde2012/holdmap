<script lang="ts">
  // Findings: toolbar, grouped list and empty states. State lives in the view-model.
  import * as api from "../../lib/api";
  import FindingsToolbar from "./FindingsToolbar.svelte";
  import FindingsList from "./FindingsList.svelte";
  import SecurityEmpty from "./SecurityEmpty.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { applyFilter, emptyFilter, findingItems, groupFindings, type FindingFilter, type GroupBy } from "../../lib/security-view";
  import type { FindingState } from "../../lib/analyzers";

  let { agents, filter = $bindable(emptyFilter()), onsetup }: { agents: { product: string; name: string }[]; filter?: FindingFilter; onsetup?: () => void } = $props();
  let groupBy = $state<GroupBy>("severity");
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  const names = $derived(new Map(agents.map((a) => [a.product, a.name])));
  const all = $derived(findingItems(insightsStore.insights, insightsStore.analyzers, names));
  const shown = $derived(applyFilter(all, filter));
  const groups = $derived(groupFindings(shown, groupBy));
  const anyOn = $derived(insightsStore.analyzers.some((a) => a.state === "approved"));
  const filtered = $derived(!!(filter.query || filter.agent || filter.severities.length));

  async function review(fingerprint: string, state: FindingState) {
    busy = fingerprint; error = null;
    try { await api.reviewFinding(fingerprint, state, state === "snoozed" ? 7 : undefined); await insightsStore.refresh(); }
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
    <FindingsToolbar bind:filter bind:groupBy {agents} shown={shown.length} />
    {#if groups.length}
      <FindingsList {groups} {groupBy} {busy} onreview={review} onrescan={securityRuns.busy ? undefined : rescan} />
    {:else}
      <SecurityEmpty kind={filtered ? "nomatch" : "clear"} onclear={() => (filter = { ...emptyFilter(), showClosed: filter.showClosed })} />
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
{/if}

<style>
  .panel { display: grid; align-content: start; gap: 12px; }
  .err { margin: 0; color: var(--danger); font-size: var(--fs-caption); }
</style>
