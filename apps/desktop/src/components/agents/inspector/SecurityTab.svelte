<script lang="ts">
  // Security: this agent's analyzer findings and skill trust alerts.
  import Icon from "../../Icon.svelte";
  import OpenFull from "./OpenFull.svelte";
  import { TRUST_LABEL } from "../../../lib/trust";
  import { SEVERITY_ORDER, agentCounts, agentFindings } from "../../../lib/analyzers";
  import { insightsStore } from "../../../lib/insights-store.svelte";
  import { trustStore } from "../../../lib/trust-store.svelte";
  import { redactHome } from "../../../lib/report-export";
  import { scopeLabel } from "../../../lib/run-view";
  import type { Agent } from "../../../lib/types";

  let { agent, base, onopen }: { agent: Agent; base: string; onopen?: () => void } = $props();

  const open = $derived(agentFindings(insightsStore.insights, agent.product));
  const reviewed = $derived(agentFindings(insightsStore.insights, agent.product, true).filter((x) => x.finding.state !== "open"));
  const severity = $derived(agentCounts(insightsStore.insights, agent.product));
  const trust = $derived(trustStore.forAgent(agent.product));
  const ran = $derived(insightsStore.analyzers.some((a) => a.state === "approved") || !!insightsStore.insights?.results.length);
</script>

<section class="sec-box" aria-labelledby="sev-head-{base}">
  <h4 id="sev-head-{base}"><Icon name="shield" size={13} />Findings for {agent.name}</h4>
  {#if !ran}
    <p class="note">No security analyzers have run yet. Turn one on in Security to check {agent.name}'s skills and packages.</p>
  {:else}
    <div class="sev-row" aria-label="Open findings by severity">
      {#each SEVERITY_ORDER.filter((s) => severity[s] > 0) as s (s)}<span class="sev-chip"><span class="sev-pill {s}">{s}</span><b>{severity[s]}</b></span>{:else}<span class="sev-chip none">No open findings</span>{/each}
    </div>
    {#if open.length}
      <ul class="findings" aria-label="Open findings">
        {#each open as x (x.finding.fingerprint)}
          <li>
            <details>
              <summary><span class="sev-pill {x.finding.severity}">{x.finding.severity}</span><span class="f-title">{x.finding.title}</span><small>{x.subject?.name ?? x.finding.subject}</small></summary>
              <dl class="f-facts">
                <div><dt>Rule</dt><dd class="mono">{x.finding.rule_id}</dd></div>
                <div><dt>Scope</dt><dd>{scopeLabel(x.subject?.scope)}</dd></div>
                <div><dt>Folder</dt><dd class="mono">{x.subject?.dir ? redactHome(x.subject.dir) : "folder unknown"}</dd></div>
                {#if x.finding.location.file}<div><dt>Source</dt><dd class="mono">{redactHome(x.finding.location.file)}{x.finding.location.line ? `:${x.finding.location.line}` : ""}</dd></div>{/if}
                {#if x.finding.fix}<div><dt>Next step</dt><dd>{x.finding.fix}</dd></div>{/if}
              </dl>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="note">No open findings for {agent.name}.</p>
    {/if}
    {#if reviewed.length}
      <details class="reviewed"><summary>Reviewed or snoozed ({reviewed.length})</summary>
        <ul class="findings">
          {#each reviewed as x (x.finding.fingerprint)}<li class="closed"><span class="sev-pill {x.finding.severity}">{x.finding.severity}</span><span class="f-title">{x.finding.title}</span><small>{x.finding.state}</small></li>{/each}
        </ul>
      </details>
    {/if}
  {/if}
</section>
{#if trust.length}
  <section class="sec-box" aria-labelledby="trust-head-{base}">
    <h4 id="trust-head-{base}"><Icon name="alert" size={13} />Skill trust <span class="sec-n">{trust.length}</span></h4>
    <ul class="rows trust">
      {#each trust as t (t.subject.id)}<li><b>{t.subject.name}</b><span class="badge warn">{TRUST_LABEL[t.state]}</span></li>{/each}
    </ul>
  </section>
{/if}
<OpenFull label="Open in Security" icon="shield" {onopen} />

<style>
  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; scroll-margin-top: 10px; }
  h4 { margin: 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .sec-n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: baseline; min-width: 0; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .findings summary:focus-visible, .reviewed summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .badge { padding: 0 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); border: 1px solid transparent; white-space: nowrap; }
  .badge.on, .badge.run { color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, var(--surface)); }
  .badge.off { color: var(--text-2); background: var(--surface-3, var(--surface)); border-color: var(--border); }
  .badge.unknown { color: var(--muted); background: var(--surface); border-color: var(--border); }
  .badge.warn { color: var(--warn); background: var(--warn-soft); }
  .trust li { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .trust b { font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .sev-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .sev-chip { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .sev-chip b { color: var(--text); font-variant-numeric: tabular-nums; }
  .sev-chip.none { color: var(--text-2); }
  .findings { gap: 4px; }
  .findings li { border-top: 1px solid var(--border); padding-top: 4px; min-width: 0; }
  .findings li:first-child { border-top: 0; padding-top: 0; }
  .findings summary { display: flex; align-items: center; gap: 6px; min-width: 0; padding: 3px 2px; cursor: pointer; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); border-radius: var(--r-sm); }
  .findings li.closed { display: flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .f-title { font-weight: var(--fw-medium); color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }
  .findings small, .findings li.closed small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  .f-facts { margin: 6px 0 4px; display: grid; gap: 4px; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface-2); }
  .f-facts div { display: grid; grid-template-columns: 74px 1fr; gap: 8px; }
  .f-facts dd { color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
  .f-facts dd.mono { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .reviewed summary { cursor: pointer; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); border-radius: var(--r-sm); }
  .reviewed .findings { margin-top: 6px; }
</style>
