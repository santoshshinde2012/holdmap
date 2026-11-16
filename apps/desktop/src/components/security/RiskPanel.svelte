<script lang="ts">
  // Top risks: a score per agent and per skill folder from open findings and skills changed since
  // trusted. Deterministic and explained; exposure isn't counted because holdmap can't confirm it.
  import Icon from "../Icon.svelte";
  import EmptyPanel from "../ui/EmptyPanel.svelte";
  import ExportActions from "../ui/ExportActions.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import * as api from "../../lib/api";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { BAND_LABEL, CHANGED_SKILL_POINTS, SEVERITY_POINTS, explain, risksCsv, risksMarkdown, sortRisks, type RiskBand, type RiskEntry, type RiskSortKey } from "../../lib/risk";
  import { reportFilename } from "../../lib/report-export";
  import { tildify } from "../../lib/format";

  let { agentNames = new Map() }: { agentNames?: Map<string, string> } = $props();

  let list = $state<RiskEntry[] | null>(null);
  let error = $state<string | null>(null);
  let scope = $state<"all" | "agent" | "folder">("all");
  let sort = $state<{ key: RiskSortKey; dir: "asc" | "desc" }>({ key: "band", dir: "desc" });
  $effect(() => {
    void insightsStore.insights; void trustStore.report;
    api.risks().then((r) => { list = r; error = null; }, (e) => (error = e instanceof Error ? e.message : String(e)));
  });
  /** Agents by name; folders with the home folder as `~` (on screen and in exports). */
  const label = (e: RiskEntry) => (e.scope === "agent" ? agentNames.get(e.key) ?? e.key : tildify(e.key));
  const shown = $derived(sortRisks((list ?? []).filter((e) => scope === "all" || e.scope === scope), sort.key, sort.dir));
  const pill: Record<RiskBand, string> = { critical: "critical", high: "high", elevated: "medium", low: "low", none: "info" };
  function by(key: RiskSortKey) { sort = { key, dir: sort.key === key && sort.dir === "desc" ? "asc" : "desc" }; }
  const aria = (key: RiskSortKey) => (sort.key === key ? (sort.dir === "asc" ? "ascending" : "descending") : "none");
</script>

<section class="risks" aria-labelledby="risk-h">
  <div class="bar">
    <h3 id="risk-h">Top risks</h3>
    <SegmentedControl bind:value={scope} label="Risk scope" options={[{ value: "all", label: "All" }, { value: "agent", label: "Agents" }, { value: "folder", label: "Folders" }]} />
    <ExportActions label="Export risks" disabled={!shown.length} copy={() => risksMarkdown(shown, Date.now(), label)}
      files={[
        { label: "Download CSV", filename: reportFilename("risks", Date.now(), "csv"), build: () => risksCsv(shown, label) },
        { label: "Download Markdown", filename: reportFilename("risks", Date.now(), "md"), build: () => risksMarkdown(shown, Date.now(), label) },
      ]} />
  </div>
  <p class="sub"><Icon name="info" size={12} />Score: {SEVERITY_POINTS.critical} per open critical, {SEVERITY_POINTS.high} high, {SEVERITY_POINTS.medium} medium, {SEVERITY_POINTS.low} low, {CHANGED_SKILL_POINTS} per skill changed since trusted. Any open critical makes it Critical. Whether a skill is reachable from outside isn't counted; holdmap can't confirm it.</p>
  {#if error}<p class="err" role="alert"><Icon name="alert" size={12} />{error}</p>{/if}
  {#if !list}
    <p class="sub" role="status">Loading…</p>
  {:else if !shown.length}
    <EmptyPanel icon="shield" tone="ok" title="No risks right now" hint="Nothing open from your analyzers, and no skill changed since you trusted it." />
  {:else}
    <div class="tw">
      <table>
        <caption class="sr-only">Top risks, {shown.length} rows</caption>
        <thead><tr>
          <th scope="col" aria-sort={aria("band")}><button type="button" onclick={() => by("band")}>Risk<Icon name={sort.key === "band" && sort.dir === "asc" ? "chevron-up" : "chevron-down"} size={10} /></button></th>
          <th scope="col" aria-sort={aria("key")}><button type="button" onclick={() => by("key")}>Agent or folder</button></th>
          <th scope="col" aria-sort={aria("scope")}><button type="button" onclick={() => by("scope")}>Kind</button></th>
          <th scope="col">Why</th>
        </tr></thead>
        <tbody>
          {#each shown as e (e.scope + e.key)}
            <tr>
              <td class="risk"><span class="sev-pill {pill[e.band]}">{BAND_LABEL[e.band]}</span><span class="pts">{e.points}</span></td>
              <td class="key">{#if e.scope === "folder"}<code>{label(e)}</code>{:else}{label(e)}{/if}</td>
              <td class="kind">{e.scope === "agent" ? "Agent" : "Folder"}</td>
              <td class="why"><span class="because">{explain(e)}</span>{#if e.top.length}<ul class="top">{#each e.top.slice(0, 3) as t (t.fingerprint)}<li><span class="sev-pill {t.severity}">{t.severity}</span><span>{t.title}</span></li>{/each}{#if e.top.length > 3}<li class="rest">+{e.top.length - 3} more</li>{/if}</ul>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  .risks { display: grid; gap: 10px; min-width: 0; }
  .bar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; min-height: 32px; }
  .bar :global(.seg) { margin-left: auto; }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .sub { display: flex; gap: 6px; align-items: baseline; margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .err { display: flex; gap: 6px; align-items: center; margin: 0; color: var(--danger); font-size: var(--fs-caption); }
  .tw { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow-x: auto; }
  table { width: 100%; min-width: 720px; table-layout: fixed; border-collapse: collapse; font-size: var(--fs-caption); }
  th, td { padding: 10px 12px; text-align: left; vertical-align: middle; border-top: 1px solid var(--border); }
  thead th { height: 36px; padding-top: 0; padding-bottom: 0; border-top: 0; color: var(--muted); font-weight: var(--fw-semibold); white-space: nowrap; }
  th:nth-child(1) { width: 148px; }
  th:nth-child(3) { width: 84px; }
  th:nth-child(4) { width: 46%; }
  th button { display: inline-flex; gap: 4px; align-items: center; min-height: 24px; padding: 0; border: 0; background: none; color: inherit; font: inherit; cursor: pointer; }
  th button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .risk { display: flex; align-items: center; gap: 8px; white-space: nowrap; }
  .pts { min-width: 3ch; color: var(--muted); font-variant-numeric: tabular-nums; text-align: right; }
  .key { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kind { color: var(--muted); }
  .why { vertical-align: top; }
  .because { display: block; min-height: 18px; line-height: var(--lh-caption); }
  .top { list-style: none; margin: 6px 0 0; padding: 0; display: grid; gap: 4px; color: var(--muted); }
  .top li { display: grid; grid-template-columns: 72px minmax(0, 1fr); gap: 8px; align-items: center; min-height: 20px; }
  .top li span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .top li.rest { display: block; padding-left: 80px; color: var(--muted); }
</style>
