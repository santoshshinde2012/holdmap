<script lang="ts">
  // An agent's Insights: open findings from approved analyzers about its skills and packages,
  // with the risk details they reported. Review hides a finding while it is unchanged.
  import Button from "../ui/Button.svelte";
  import * as api from "../../lib/api";
  import { agentFindings, findingWhere, subjectDetails } from "../../lib/analyzers";
  import { insightsStore } from "../../lib/insights-store.svelte";

  let { product, onsettings }: { product: string; onsettings?: () => void } = $props();
  let showAll = $state(false);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  const rows = $derived(agentFindings(insightsStore.insights, product, showAll));
  const anyRun = $derived((insightsStore.insights?.results ?? []).some((r) => r.subjects.some((s) => s.agents.includes(product))));
  const subjects = $derived.by(() => {
    const seen = new Map<string, { name: string; details: { label: string; value: string }[] }>();
    for (const r of insightsStore.insights?.results ?? []) {
      for (const s of r.subjects) if (s.agents.includes(product)) {
        const details = subjectDetails(r, s.id);
        if (details.length) seen.set(`${r.analyzer}:${s.id}`, { name: s.name, details });
      }
    }
    return [...seen.values()];
  });

  async function review(fingerprint: string, state: "reviewed" | "snoozed" | "open") {
    busy = fingerprint;
    error = null;
    try { await api.reviewFinding(fingerprint, state, state === "snoozed" ? 7 : undefined); await insightsStore.refresh(); }
    catch (e) { error = String(e); }
    finally { busy = null; }
  }
</script>

<div class="insights">
  {#if !anyRun}
    <p class="muted">No analyzer has looked at this agent yet.{#if onsettings} <button type="button" class="link" onclick={onsettings}>Set up analyzers</button>{/if}</p>
  {:else}
    {#if rows.length === 0}<p class="muted">No open findings.</p>{/if}
    <ul>
      {#each rows as { finding: f, subject } (f.fingerprint)}
        <li class="sev-{f.severity}" class:closed={f.state !== "open"}>
          <span class="sev">{f.severity}</span>
          <span class="t">{f.title}</span>
          <span class="where">{subject?.name ?? f.subject}{#if findingWhere(f)} · <code>{findingWhere(f)}</code>{/if}</span>
          {#if f.fix}<span class="fix">{f.fix}</span>{/if}
          <span class="acts">
            {#if f.state === "open"}
              <Button size="xs" variant="ghost" loading={busy === f.fingerprint} onclick={() => review(f.fingerprint, "reviewed")}>Mark reviewed</Button>
              <Button size="xs" variant="ghost" disabled={busy === f.fingerprint} onclick={() => review(f.fingerprint, "snoozed")}>Snooze 7 days</Button>
            {:else}
              <span class="muted">{f.state}</span>
              <Button size="xs" variant="ghost" loading={busy === f.fingerprint} onclick={() => review(f.fingerprint, "open")}>Reopen</Button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
    {#if subjects.length}
      <dl class="details">
        {#each subjects as s}<div><dt>{s.name}</dt><dd>{s.details.map((d) => `${d.label}: ${d.value}`).join(" · ")}</dd></div>{/each}
      </dl>
    {/if}
    <label class="all"><input type="checkbox" bind:checked={showAll} /> Show reviewed and snoozed</label>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</div>

<style>
  .insights { display: grid; gap: var(--sp-2); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  li { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 2px var(--sp-2); padding: 6px 8px; border: 1px solid var(--border); border-left: 3px solid var(--warn); border-radius: var(--r-md); background: var(--surface); }
  li.sev-high, li.sev-critical { border-left-color: var(--danger); }
  li.sev-low, li.sev-info { border-left-color: var(--border-strong); }
  li.closed { opacity: 0.7; }
  .sev { font-size: var(--fs-caption); font-weight: var(--fw-semibold); text-transform: capitalize; color: var(--text-2); }
  .t { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); }
  .where, .fix, .acts { grid-column: 2; font-size: var(--fs-caption); color: var(--muted); overflow-wrap: anywhere; }
  .fix { color: var(--text-2); }
  .acts { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  code { font-family: var(--mono); }
  .details { margin: 0; display: grid; gap: 2px; font-size: var(--fs-caption); }
  .details div { display: flex; gap: var(--sp-2); }
  .details dt { color: var(--text-2); font-weight: var(--fw-medium); }
  .details dd { margin: 0; color: var(--muted); }
  .muted { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .link { border: 0; background: none; padding: 0; color: var(--accent-text, var(--accent)); font: inherit; cursor: pointer; text-decoration: underline; }
  .all { display: flex; gap: 6px; align-items: center; font-size: var(--fs-caption); color: var(--muted); }
  .err { margin: 0; color: var(--danger); font-size: var(--fs-caption); }
</style>
