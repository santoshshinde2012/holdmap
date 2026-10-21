<script lang="ts" module>
  // Remembered across remounts (the Agents layout swaps when the window narrows).
  let lastTab: "findings" | "analyzers" | "graph" | null = null;
  let lastPipeline: string | null = null;
</script>

<script lang="ts">
  // The Insights tab: findings from approved analyzers for every agent, including agents that
  // aren't running when the opt-in inventory has read their default configuration. Pick an agent
  // on the left; its findings, risk details and review actions are on the right.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import AgentInsights from "./AgentInsights.svelte";
  import AnalyzerGallery from "./AnalyzerGallery.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import { loadAnalyzerGraphs, type AnalyzerGraphs } from "./graph/load-graphs";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { inventories } from "../../lib/agent-config";
  import { agentCounts, ago, countsText, total, worst } from "../../lib/analyzers";
  import type { AgentsReport } from "../../lib/types";

  let {
    report,
    selectedAgent = null,
    inventory = false,
    onselectagent,
    onsettings,
    oninventory,
  }: {
    report: AgentsReport | null;
    /** The app's selected running agent, kept in step with this list. */
    selectedAgent?: string | null;
    /** Whether the opt-in inventory is on (needed for skills and agents that aren't running). */
    inventory?: boolean;
    onselectagent?: (id: string | null) => void;
    onsettings?: () => void;
    oninventory?: () => void;
  } = $props();

  /** One row per product: running instances share their findings. */
  const rows = $derived.by(() => {
    const seen = new Map<string, { product: string; name: string; running: boolean; id: string | null }>();
    for (const i of inventories(report)) if (!seen.has(i.product)) seen.set(i.product, { product: i.product, name: i.name, running: i.running, id: i.id });
    return [...seen.values()].map((r) => ({ ...r, counts: agentCounts(insightsStore.insights, r.product) }));
  });
  let picked = $state<string | null>(null);
  const fromApp = $derived(report?.agents.find((a) => a.id === selectedAgent)?.product ?? null);
  const current = $derived(rows.find((r) => r.product === (picked ?? fromApp)) ?? rows.find((r) => total(r.counts) > 0) ?? rows[0] ?? null);
  const approved = $derived(insightsStore.analyzers.filter((a) => a.state === "approved"));
  const lastRun = $derived(Math.max(0, ...(insightsStore.insights?.results ?? []).map((r) => r.run_at_ms)));
  const open = $derived(insightsStore.insights ? total(insightsStore.insights.open) : 0);

  // With nothing approved yet, open on the gallery so the analyzers are easy to find.
  let tab = $state<"findings" | "analyzers" | "graph">(lastTab ?? (insightsStore.analyzers.some((a) => a.state === "approved") ? "findings" : "analyzers"));
  let pipeline = $state<string | null>(lastPipeline);
  $effect(() => { lastTab = tab; lastPipeline = pipeline; });
  let graphs = $state<AnalyzerGraphs | null>(null);
  let graphError = $state(false);
  $effect(() => {
    if (tab !== "graph" || graphs) return;
    graphError = false;
    loadAnalyzerGraphs().then((g) => (graphs = g), (e) => { console.error("Couldn't load the analyzer graphs", e); graphError = true; });
  });
  const pipelineView = $derived(insightsStore.analyzers.find((a) => a.id === pipeline) ?? null);

  function pick(r: { product: string; id: string | null }) {
    picked = r.product;
    if (r.id) onselectagent?.(r.id);
  }
</script>

<section id="agent-insights" class="insights-ws" aria-label="Security workspace">
  <header class="head">
    <div class="title">
      <h2><Icon name="shield" size={15} />Security</h2>
      <p class="sub">
        {#if !approved.length}No analyzer is on. Nothing runs until you turn one on.
        {:else}{open ? `${open} open ${open === 1 ? "finding" : "findings"}` : "No open findings"} from {approved.length} {approved.length === 1 ? "analyzer" : "analyzers"}{#if lastRun} · last run {ago(lastRun)}{/if}{/if}
      </p>
    </div>
    <div class="tools">
      <SegmentedControl bind:value={tab} label="Security view" options={[{ value: "findings", label: "Findings" }, { value: "analyzers", label: "Analyzers", icon: "shield" }, { value: "graph", label: "How it works", icon: "branch" }]} />
      {#if onsettings}<Button size="sm" variant="ghost" icon="sliders" onclick={onsettings}>Settings…</Button>{/if}
    </div>
  </header>

  {#if !inventory}
    <p class="note"><Icon name="info" size={12} />Agent inventory is off, so stopped agents and their skills aren't scanned. {#if oninventory}<button type="button" class="link" onclick={oninventory}>Turn on inventory</button>{/if}</p>
  {/if}

  {#if tab === "analyzers"}
    <AnalyzerGallery {onsettings} />
  {:else if tab === "graph"}
    <div class="graphs">
      {#if !graphs}
        <p class="sub" role="status">{graphError ? "Couldn't load the graphs. Reopen this tab to try again." : "Loading the graphs…"}</p>
      {:else if pipelineView}
        <div class="ghead"><h3>How {pipelineView.name} works</h3><Button size="sm" variant="ghost" icon="arrow-left" onclick={() => (pipeline = null)}>All analyzers</Button></div>
        <graphs.AnalyzerPipeline view={pipelineView} height={420} />
      {:else}
        <p class="sub">Which analyzer covers which agent. Select one to see its pipeline.</p>
        <graphs.AnalyzerOverview {report} height={440} onpipeline={(id) => (pipeline = id)} />
      {/if}
    </div>
  {:else if !rows.length}
    <div class="empty"><Icon name="bot" size={28} /><p>No agents to show</p><span>Running agents, and agents with readable configuration, appear here.</span></div>
  {:else}
    <div class="body">
      <nav class="list" aria-label="Agents with insights">
        <ul>
          {#each rows as r (r.product)}
            {@const level = worst(r.counts)}
            <li>
              <button type="button" class="row sev-{level ?? 'none'}" aria-current={current?.product === r.product ? "true" : undefined} onclick={() => pick(r)}>
                <span class="name">{r.name}</span>
                {#if !r.running}<span class="tag">not running</span>{/if}
                <span class="counts">{total(r.counts) ? countsText(r.counts) : "no open findings"}</span>
              </button>
            </li>
          {/each}
        </ul>
      </nav>
      {#if current}
        <div class="detail">
          <h3>{current.name}{#if !current.running} <span class="tag">not running · from its configuration</span>{/if}</h3>
          <AgentInsights product={current.product} {onsettings} />
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .insights-ws { display: grid; grid-template-rows: auto auto minmax(0, 1fr); gap: var(--sp-3); height: 100%; min-height: 0; padding: var(--sp-4); box-sizing: border-box; overflow: auto; }
  .tools { display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; }
  .graphs { display: grid; align-content: start; gap: var(--sp-2); min-width: 0; }
  .ghead { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); }
  .head { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--sp-3); }
  h2 { display: flex; align-items: center; gap: 6px; margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); }
  .sub { margin: 2px 0 0; color: var(--muted); font-size: var(--fs-caption); }
  .note { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin: 0; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text-2); font-size: var(--fs-caption); }
  .body { display: grid; grid-template-columns: minmax(180px, 260px) minmax(0, 1fr); gap: var(--sp-4); min-height: 0; }
  .list ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 6px; width: 100%; padding: 6px 8px; border: 1px solid var(--border); border-left: 3px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); text-align: left; font: inherit; cursor: pointer; }
  .row.sev-medium { border-left-color: var(--warn); }
  .row.sev-high, .row.sev-critical { border-left-color: var(--danger); }
  .row[aria-current="true"] { background: var(--accent-soft); border-color: var(--accent); }
  .row:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .name { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .counts { grid-column: 1 / -1; color: var(--muted); font-size: var(--fs-caption); }
  .tag { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-regular); }
  .detail { display: grid; align-content: start; gap: var(--sp-2); min-width: 0; }
  h3 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .empty { display: grid; justify-items: center; gap: 4px; padding: var(--sp-6); color: var(--muted); text-align: center; }
  .empty p { margin: 0; color: var(--text); font-weight: var(--fw-medium); }
  .link { border: 0; background: none; padding: 0; color: var(--accent-text, var(--accent)); font: inherit; cursor: pointer; text-decoration: underline; }
  @media (max-width: 720px) { .body { grid-template-columns: minmax(0, 1fr); } }
</style>
