<script lang="ts">
  // Home: the same agents as Cards or as a Map (radial footprint plus a collapsible timeline),
  // with "Needs you" beside both. Hover and selection are shared across cards, map, timeline and
  // attention items, and survive a layout switch. Esc clears the selection.
  import { insightsStore } from "../../lib/insights-store.svelte";
  import * as api from "../../lib/api";
  import Icon from "../Icon.svelte";
  import { SegmentedControl } from "../ui";
  import EmptyPanel from "../ui/EmptyPanel.svelte";
  import ExportActions from "../ui/ExportActions.svelte";
  import { homeReport, homeSummaryText, reportFilename } from "../../lib/report-export";
  import HomeSummary, { type SummaryStat } from "./HomeSummary.svelte";
  import AgentCards from "./AgentCards.svelte";
  import MachineMap from "./MachineMap.svelte";
  import MapList from "./MapList.svelte";
  import NeedsList from "./NeedsList.svelte";
  import AgentLanes from "./AgentLanes.svelte";
  import PortMenu from "./PortMenu.svelte";
  import { attention, attentionAgent, hotAgents, overview, type AttentionItem } from "../../lib/home";
  import { radialLayout, type FootprintNode } from "../../lib/machine-map";
  import { lanes } from "../../lib/agent-lanes";
  import { agentColors } from "../../lib/agents";
  import { HOME_LAYOUTS, HOME_LAYOUT_LABEL, type HomeLayout } from "../../lib/home-layout";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  type Section = "overview" | "capabilities" | "insights";
  let { entries, report, inventory = false, now = Date.now(), layout = "cards", onlayout, timeline = true, ontimeline, onopenagent, onopenentry, onview, oninventory, onstopentry, onstopagent, selectedAgent = null, onselectagent }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    /** Time of the latest scan. */
    now?: number;
    /** Cards or Map; the app shell persists it and binds the V shortcut. */
    layout?: HomeLayout;
    onlayout?: (l: HomeLayout) => void;
    /** Whether the Map's timeline band is open; also persisted by the app shell. */
    timeline?: boolean;
    ontimeline?: (open: boolean) => void;
    onopenagent: (id: string, section?: Section) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    oninventory?: () => void;
    /** Both open the stop plan for review; neither stops anything directly. */
    onstopentry?: (id: string) => void;
    onstopagent?: (id: string) => void;
    /** The app's shared selected agent, so a pick here is still selected on Agents. */
    selectedAgent?: string | null;
    onselectagent?: (id: string | null) => void;
  } = $props();

  const o = $derived(overview(entries, report));
  // The TypeScript mirror paints instantly; in the app the Rust rules then replace it.
  const quick = $derived(attention(entries, report, insightsStore.insights));
  let decided = $state<{ from: AttentionItem[]; items: AttentionItem[] } | null>(null);
  const needs = $derived(decided && decided.from === quick ? decided.items : quick);
  $effect(() => {
    const from = quick;
    let live = true;
    api.needs(entries, report, insightsStore.insights).then(
      (items) => { if (live) decided = { from, items }; },
      () => {},
    );
    return () => { live = false; };
  });
  /** Remote-host groups the user expanded on the map. */
  let expanded = $state<ReadonlySet<string>>(new Set());
  const groups = $derived(radialLayout(report?.agents ?? [], undefined, undefined, undefined, undefined, expanded));
  const rows = $derived(lanes(report, entries, now));
  const hot = $derived(hotAgents(needs));
  const colors = $derived(agentColors(report?.agents ?? []));
  const LAYOUT_ICON: Record<HomeLayout, string> = { cards: "layers", map: "radar" };
  const layoutOptions = HOME_LAYOUTS.map((l) => ({ value: l, label: HOME_LAYOUT_LABEL[l], icon: LAYOUT_ICON[l], title: `${HOME_LAYOUT_LABEL[l]} · V` }));

  let hovered = $state<string | null>(null);
  let localSelected = $state<string | null>(null);
  const selected = $derived(onselectagent ? selectedAgent : localSelected);
  function setSelected(id: string | null) { if (onselectagent) onselectagent(id); else localSelected = id; }
  const active = $derived(hovered ?? selected);
  const activeNeeds = $derived(active ? new Set(needs.filter((n) => attentionAgent(n, report) === active).map((n) => n.id)) : null);

  const stats = $derived<SummaryStat[]>([
    { id: "agents", value: o.agents, label: "agents", go: () => onview("agents") },
    { id: "ports", value: o.ports, label: "listening", go: () => onview("list") },
    { id: "dev", value: o.devServers, label: "dev servers", go: () => onview("list") },
    { id: "exposed", value: o.exposed, label: "exposed", tone: o.exposed ? "warn" : undefined, go: () => onview("list") },
    ...(o.mcpConfigured !== null ? [{ id: "mcp", value: o.mcpRunning, of: o.mcpConfigured, label: "MCP running", go: () => onview("agents") } satisfies SummaryStat] : []),
  ]);
  const showInventory = $derived(!inventory && !!oninventory && (report?.agents ?? []).some((a) => !a.capabilities));

  function open(t: FootprintNode["target"]) {
    if (t.view === "expand") { const next = new Set(expanded); if (!next.delete(t.groupId)) next.add(t.groupId); expanded = next; }
    else if (t.view === "list") onopenentry(t.entryId);
    else onopenagent(t.agentId);
  }
  // Analyzer findings open the agent's Insights tab; everything else opens the agent or port.
  function openItem(i: AttentionItem) { if (i.rule === "analyzer" && i.target.view === "agents") onopenagent(i.target.agentId, "insights"); else open(i.target); }
  function stopItem(i: AttentionItem) { if (i.stop?.kind === "entry") onstopentry?.(i.stop.entryId); else if (i.stop) onstopagent?.(i.stop.agentId); }
  let portMenu = $state<{ entry: PortEntry; anchor: HTMLElement | SVGElement } | null>(null);
  function port(entryId: string, anchor: HTMLElement | SVGElement) {
    const entry = entries?.find((e) => e.id === entryId);
    if (entry) portMenu = { entry, anchor }; else onopenentry(entryId);
  }
  function select(id: string) { setSelected(selected === id ? null : id); }
  function onkeydown(e: KeyboardEvent) { if (e.key === "Escape" && selected) { setSelected(null); e.stopPropagation(); } }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="home" id="home-overview" role="region" aria-label="Overview" {onkeydown}>
  <header class="top">
    <HomeSummary {stats} />
    <div class="tools">
      {#if layout === "map"}
        <span class="legend" aria-label="Legend">
          <span><i class="k project"></i>Project</span><span><i class="k port"></i>Port</span><span><i class="k mcp"></i>MCP</span><span><i class="k remote"></i>Remote</span><span><i class="k warn"></i>Needs you</span>
        </span>
      {/if}
      <ExportActions label="Export overview" disabled={!report || !entries} copy={() => homeSummaryText(o, needs, Date.now())}
        files={[{ label: "Download JSON", filename: reportFilename("overview", Date.now(), "json"), build: () => homeReport(o, needs, report, Date.now()) }]} />
      {#if showInventory}<button type="button" class="inv" onclick={oninventory}><Icon name="plug" size={12} />Read MCP config</button>{/if}
      <SegmentedControl value={layout} options={layoutOptions} label="Home layout" onchange={(l) => onlayout?.(l)} />
    </div>
  </header>

  <div class="body" class:map={layout === "map"}>
    {#key layout}
      <div class="main">
        {#if layout === "cards"}
          {#if report}
            <AgentCards {report} {entries} {needs} {colors} {now} {selected} {active} onhover={(id) => (hovered = id)} onselect={select}
              onopen={(id) => onopenagent(id)} onport={port} onstop={onstopagent} oninventory={inventory ? undefined : oninventory} />
          {:else}
            <p class="clear wait" aria-busy="true">…</p>
          {/if}
        {:else}
          <div class="stage">
            <div class="map-wrap">
              <MachineMap {groups} {colors} platform={report?.platform ?? ""} ports={o.ports} {active} {selected} {hot} onhover={(id) => (hovered = id)} onselect={select} onopen={open} onport={port} />
            </div>
            <div class="map-list"><MapList {groups} {colors} {active} onhover={(id) => (hovered = id)} onopenagent={(id) => onopenagent(id)} onport={port} /></div>
          </div>
        {/if}
      </div>
    {/key}
    <aside class="panel" aria-labelledby="home-needs">
      <h2 id="home-needs">Needs you{#if needs.length}<span class="count">{needs.length}</span>{/if}</h2>
      {#if needs.length}
        <div class="needs-scroll">
          <NeedsList items={needs} lit={activeNeeds} onopen={openItem} onstop={stopItem} onhover={(i) => (hovered = i ? attentionAgent(i, report) : null)} />
        </div>
      {:else if entries && report}
        <EmptyPanel compact tone="ok" icon="check" title="All clear" hint="Nothing needs you right now." />
      {:else}
        <p class="clear" aria-busy="true">…</p>
      {/if}
    </aside>
  </div>

  {#if layout === "map"}
    {#if rows.length}
      <AgentLanes lanes={rows} {now} {colors} {active} open={timeline} ontoggle={() => ontimeline?.(!timeline)} onhover={(id) => (hovered = id)} onselect={select} onopenagent={(id) => onopenagent(id)} onport={port} />
    {:else}
      <p class="clear lanes-empty" aria-busy={!report}>{report ? "No agents" : "…"}</p>
    {/if}
  {/if}
  {#each portMenu ? [portMenu] : [] as m (m.entry.id)}
    <PortMenu entry={m.entry} anchor={m.anchor} ondetails={() => onopenentry(m.entry.id)} onstop={() => onstopentry?.(m.entry.id)} onclose={() => (portMenu = null)} />
  {/each}
</div>

<style>
  /* max-content rows: the header sets a min-height, and auto rows would hold it to that once the
     body overflows, letting a wrapped summary run under the panel on narrow screens. */
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-4) var(--sp-5) var(--sp-6); display: grid; grid-auto-rows: max-content; gap: var(--sp-4); align-content: start; width: 100%; max-width: 1680px; margin: 0 auto; }
  .top { display: flex; align-items: center; gap: var(--sp-2) var(--sp-4); flex-wrap: wrap; min-height: 30px; }
  .tools { margin-left: auto; display: flex; align-items: center; gap: var(--sp-3); flex-wrap: wrap; justify-content: flex-end; }
  .legend { display: flex; gap: var(--sp-3); flex-wrap: wrap; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .k { display: inline-block; width: 9px; height: 9px; border: 1.5px solid var(--text-2); }
  .k.project { border-radius: 2.5px; } .k.port { border-radius: 50%; background: var(--text-2); } .k.mcp { transform: rotate(45deg) scale(0.85); border-radius: 1.5px; } .k.remote { border-radius: 50%; }
  .k.warn { border-color: var(--warn); border-radius: 50%; box-shadow: 0 0 0 3px var(--warn-soft); }
  .inv { display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--accent); font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .inv:hover { background: var(--accent-softer); }
  .inv:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .body { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: var(--sp-4); align-items: start; }
  .body.map { align-items: stretch; height: clamp(340px, 44vh, 560px); }
  .main { min-width: 0; min-height: 0; height: 100%; animation: view-in 180ms ease both; }
  @keyframes view-in { from { opacity: 0; transform: translateY(3px); } }
  @media (prefers-reduced-motion: reduce) { .main { animation: none; } }
  .body:not(.map) .panel { position: sticky; top: 0; max-height: calc(100vh - 220px); }
  .wait { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .stage { height: 100%; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; min-height: 0; overflow: clip; }
  .map-wrap { width: 100%; height: 100%; padding: var(--sp-2); }
  .map-list { display: none; }
  .panel { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); display: grid; grid-template-rows: auto minmax(0, 1fr); min-height: 0; }
  h2 { margin: 0; padding: var(--sp-3) var(--sp-3) var(--sp-2); display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); border-bottom: 1px solid var(--border); }
  .count { padding: 0 7px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; }
  .needs-scroll { overflow-y: auto; min-height: 0; }
  .clear { margin: 0; padding: var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .clear :global(svg) { color: var(--ok); }
  .lanes-empty { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  @media (max-width: 1100px) { .body { grid-template-columns: minmax(0, 1fr) 280px; } .body.map { height: clamp(300px, 38vh, 480px); } }
  @media (max-width: 860px) {
    .body, .body.map { grid-template-columns: minmax(0, 1fr); height: auto; }
    .body.map .stage { height: 420px; }
    .body:not(.map) .panel { position: static; }
    .panel { order: -1; }
    .panel { max-height: none !important; }
  }
  @media (max-width: 760px) {
    .home { padding: var(--sp-3); gap: var(--sp-3); }
    .legend { display: none; }
    .body.map .stage { height: auto; }
    .map-wrap { display: none; }
    .map-list { display: block; }
  }
</style>
