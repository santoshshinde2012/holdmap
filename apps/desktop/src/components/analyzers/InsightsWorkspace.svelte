<script lang="ts">
  // The Security tab: a summary header, findings (grouped, filterable, expandable inline), the
  // analyzer gallery, and the pipeline graphs. Shaping lives in lib/security-view.ts; runs in
  // lib/security-runs.svelte.ts. Analyzers stay off until approved in their permissions sheet.
  import { untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import AnalyzerGallery from "./AnalyzerGallery.svelte";
  import SecuritySummary from "../security/SecuritySummary.svelte";
  import ExportActions from "../ui/ExportActions.svelte";
  import { reportFilename, securityCsv, securityMarkdown, securityReport, securitySarif, securitySummaryText } from "../../lib/report-export";
  import SkillExplainer from "../security/SkillExplainer.svelte";
  import PipelineSubjects from "../security/PipelineSubjects.svelte";
  import RunHistory from "../security/RunHistory.svelte";
  import { securityTab, type SecurityTab } from "../../lib/security-tab.svelte";
  import FindingsPanel from "../security/FindingsPanel.svelte";
  import { loadAnalyzerGraphs, type AnalyzerGraphs } from "./graph/load-graphs";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { inventories } from "../../lib/agent-config";
  import { applyFilter, emptyFilter, findingItems, securitySummary, type FindingFilter } from "../../lib/security-view";
  import type { Severity } from "../../lib/analyzers";
  import type { AgentsReport } from "../../lib/types";

  let { report, selectedAgent = null, inventory = false, onselectagent, onsettings, oninventory }: {
    report: AgentsReport | null;
    /** The app's selected running agent; picking it in the agent filter keeps the app in step. */
    selectedAgent?: string | null;
    /** Whether the opt-in inventory is on (needed for skills and agents that aren't running). */
    inventory?: boolean;
    onselectagent?: (id: string | null) => void;
    onsettings?: () => void;
    oninventory?: () => void;
  } = $props();

  const inv = $derived(inventories(report));
  const agents = $derived([...new Map(inv.map((i) => [i.product, { product: i.product, name: i.name, id: i.id }])).values()]);
  const skillsTotal = $derived(new Set(inv.flatMap((i) => i.capabilities?.skills.map((s) => s.id) ?? [])).size);
  const summary = $derived(securitySummary(insightsStore.insights, insightsStore.analyzers, agents, skillsTotal));
  const allItems = $derived(findingItems(insightsStore.insights, insightsStore.analyzers, new Map(agents.map((a) => [a.product, a.name]))));
  const approved = $derived(insightsStore.analyzers.filter((a) => a.state === "approved"));

  if (!securityTab.value) securityTab.value = insightsStore.analyzers.some((a) => a.state === "approved") ? "findings" : "analyzers";
  const tab = $derived<SecurityTab>(securityTab.value ?? "findings");
  const setTab = (t: SecurityTab) => (securityTab.value = t);
  const pipeline = $derived(securityTab.pipeline);
  // Opening Security from an agent (its card's "Open Security") scopes the findings to it.
  const fromApp = $derived(report?.agents.find((a) => a.id === selectedAgent)?.product ?? null);
  let filter = $state<FindingFilter>({ ...emptyFilter(), agent: untrack(() => fromApp) });
  /** What an export carries: exactly what the filter shows on screen. */
  const exportable = $derived(applyFilter(allItems, filter));
  const included = $derived(inclusion(filter));
  function inclusion(f: FindingFilter): string {
    const parts = [f.showClosed ? "open and reviewed findings" : "open findings"];
    if (f.severities.length) parts.push(`severity ${f.severities.join("/")}`);
    if (f.agent) parts.push(`agent ${agents.find((a) => a.product === f.agent)?.name ?? f.agent}`);
    if (f.query) parts.push(`matching "${f.query}"`);
    return `Only what's on screen: ${parts.join(", ")}`;
  }
  let lastAgent: string | null = untrack(() => fromApp);
  $effect(() => {
    const a = filter.agent;
    if (a === lastAgent) return;
    lastAgent = a;
    const id = agents.find((x) => x.product === a)?.id;
    if (id && id !== selectedAgent) onselectagent?.(id);
  });

  let graphs = $state<AnalyzerGraphs | null>(null);
  let graphError = $state(false);
  $effect(() => {
    if (tab !== "graph" || graphs) return;
    graphError = false;
    loadAnalyzerGraphs().then((g) => (graphs = g), (e) => { console.error("Couldn't load the analyzer graphs", e); graphError = true; });
  });
  const pipelineView = $derived(insightsStore.analyzers.find((a) => a.id === pipeline) ?? null);

  function toggleSeverity(s: Severity) {
    const has = filter.severities.includes(s);
    filter = { ...filter, severities: has ? filter.severities.filter((x) => x !== s) : [...filter.severities, s] };
    if (tab !== "results") setTab("findings");
  }
  function showPipeline(id: string) { securityTab.pipeline = id; setTab("graph"); }
</script>

<section id="agent-insights" class="ws" aria-label="Security workspace">
  <header class="head">
    <h2><Icon name="shield" size={16} />Security</h2>
    <div class="tools">
      <SegmentedControl bind:value={() => tab, setTab} label="Security view" options={[{ value: "findings", label: "Findings", count: summary.open || null, countLabel: `${summary.open} open` }, { value: "results", label: "Results", icon: "list" }, { value: "analyzers", label: "Analyzers", icon: "shield" }, { value: "graph", label: "Pipeline", icon: "branch" }]} />
      {#if tab === "findings" || tab === "results"}
        <ExportActions label="Export findings" disabled={!insightsStore.insights} copy={() => securitySummaryText(exportable, summary, Date.now())}
          files={[
            { label: "Download JSON", filename: reportFilename("security", Date.now(), "json"), build: () => securityReport(exportable, summary, Date.now(), included) },
            { label: "Download SARIF 2.1.0", filename: reportFilename("security", Date.now(), "sarif"), build: () => securitySarif(exportable, undefined, included) },
            { label: "Download CSV", filename: reportFilename("security", Date.now(), "csv"), build: () => securityCsv(exportable) },
            { label: "Download Markdown", filename: reportFilename("security", Date.now(), "md"), build: () => securityMarkdown(exportable, summary, Date.now(), included) },
          ]} />
      {/if}
      <Button size="sm" variant="primary" icon="refresh" disabled={!approved.length} loading={securityRuns.busy} onclick={() => securityRuns.start(approved)} title={approved.length ? undefined : "Turn on an analyzer first"}>Rescan</Button>
      {#if onsettings}<IconButton icon="sliders" label="Analyzer settings" size="sm" onclick={onsettings} />{/if}
    </div>
  </header>

  <SecuritySummary {summary} active={filter.severities} ontoggle={toggleSeverity} />
  <SkillExplainer />

  {#if !inventory}
    <p class="note"><Icon name="info" size={12} />Stopped agents aren't scanned.{#if oninventory}<button type="button" class="link" onclick={oninventory}>Turn on inventory</button>{/if}</p>
  {/if}

  {#if tab === "analyzers"}
    <AnalyzerGallery {onsettings} onpipeline={showPipeline} />
  {:else if tab === "results"}
    <p class="note" data-export-scope><Icon name="info" size={12} />Exports carry {included.replace("Only what's on screen: ", "only what's on screen — ")} ({exportable.length}).</p>
    <FindingsPanel {agents} bind:filter view="table" onsetup={() => setTab("analyzers")} />
    <RunHistory analyzers={insightsStore.analyzers} version={securityRuns.finishedAt ?? 0} />
  {:else if tab === "graph"}
    <div class="graphs">
      {#if !graphs}
        <p class="sub" role="status">{graphError ? "Couldn't load the graphs. Reopen this tab to try again." : "Loading…"}</p>
      {:else if pipelineView}
        <div class="ghead"><h3>{pipelineView.name}</h3><Button size="sm" variant="ghost" icon="arrow-left" onclick={() => (securityTab.pipeline = null)}>All analyzers</Button></div>
        <div class="frame"><graphs.AnalyzerPipeline view={pipelineView} height={420} /></div>
        <PipelineSubjects analyzer={pipelineView.id} items={allItems} />
      {:else}
        <div class="ghead"><h3>Coverage</h3><span class="sub">Select an analyzer to see its pipeline</span></div>
        <div class="frame"><graphs.AnalyzerOverview {report} height={440} onpipeline={(id) => (securityTab.pipeline = id)} /></div>
      {/if}
    </div>
  {:else}
    <FindingsPanel {agents} bind:filter onsetup={() => setTab("analyzers")} />
  {/if}
</section>

<style>
  .ws { --scroll-pad-bottom: 20px; display: grid; grid-template-columns: minmax(0, 1fr); grid-auto-rows: max-content; align-content: start; gap: 16px; height: 100%; min-height: 0; padding: 20px 24px; box-sizing: border-box; overflow: auto; max-width: 1280px; margin: 0 auto; width: 100%; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; }
  h2 { display: flex; align-items: center; gap: 8px; margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); }
  .tools { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .note { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .link { border: 0; background: none; padding: 0 2px; min-height: 24px; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  .graphs { display: grid; align-content: start; gap: 10px; min-width: 0; }
  .ghead { display: flex; align-items: baseline; gap: 10px; }
  .ghead :global(.ui-btn) { margin-left: auto; }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .frame { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
</style>
