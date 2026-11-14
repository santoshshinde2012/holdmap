<script lang="ts">
  // Home: the same agents as Cards, as a Map (radial footprint plus a collapsible timeline) or
  // as a 3D machine (loaded on demand), with "Needs you" beside each. Hover and selection are shared across cards, map, timeline and
  // attention items, and survive a layout switch. Esc clears the selection.
  import { insightsStore } from "../../lib/insights-store.svelte";
  import ConsentGate from "../ui/ConsentGate.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { securityTab } from "../../lib/security-tab.svelte";
  import * as api from "../../lib/api";
  import Icon from "../Icon.svelte";
  import { Button, CollapsiblePanel, IconButton, SegmentedControl } from "../ui";
  import type { Snippet } from "svelte";
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
  import { attention, attentionAgent, hotAgents, needsByAgent, overview, type AttentionItem } from "../../lib/home";
  import { radialLayout, type FootprintNode } from "../../lib/machine-map";
  import { lanes } from "../../lib/agent-lanes";
  import { agentColors } from "../../lib/agents";
  import { HOME_LAYOUTS, HOME_LAYOUT_LABEL, type HomeLayout } from "../../lib/home-layout";
  import { prefGetBoolean, prefSet } from "../../lib/prefs";
  import type { ActivitySnapshot, RecordingStatus } from "../../lib/agent-flow/model";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  type Section = "overview" | "capabilities" | "insights";
  let { entries, report, inventory = false, now = Date.now(), layout = "cards", onlayout, timeline = true, ontimeline, singleKeys = true, activity = null, recording = null, onopenagent, onopenentry, onview, onsecurity, oninventory, onstopentry, onstopagent, selectedAgent = null, onselectagent, agentsError = null, onretryagents }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    /** Time of the latest scan. */
    now?: number;
    /** Cards, Map or 3D; the app shell persists it and binds the V shortcut. */
    layout?: HomeLayout;
    onlayout?: (l: HomeLayout) => void;
    /** Whether the Map's timeline band is open; also persisted by the app shell. */
    timeline?: boolean;
    ontimeline?: (open: boolean) => void;
    /** The app's single-key shortcuts setting, for the 3D view's E, X, T and F. */
    singleKeys?: boolean;
    /** Opt-in recorded calls for the 3D view's calls lane. */
    activity?: ActivitySnapshot | null;
    recording?: RecordingStatus | null;
    onopenagent: (id: string, section?: Section) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    /** Opens Security → Findings (the skill-security tile). */
    onsecurity?: () => void;
    oninventory?: () => void;
    /** Both open the stop plan for review; neither stops anything directly. */
    onstopentry?: (id: string) => void;
    onstopagent?: (id: string) => void;
    /** The app's shared selected agent, so a pick here is still selected on Agents. */
    selectedAgent?: string | null;
    onselectagent?: (id: string | null) => void;
    /** Why the agents lookup failed, while there's no report to show. */
    agentsError?: string | null;
    onretryagents?: () => void;
  } = $props();

  const o = $derived(overview(entries, report));
  // The TypeScript mirror paints instantly; in the app the Rust rules then replace it.
  $effect(() => { if (!trustStore.report) void trustStore.load(); });
  const trustAlerts = $derived(trustStore.alerting);
  const trustChanged = $derived(trustAlerts.filter((s) => s.state === "changed").length);
  function openSkills() { onsecurity?.(); securityTab.value = "skills"; }
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
  /** Nothing observed at all: the first minute on a quiet machine. One calm message instead of empty cards or a bare map. */
  const idle = $derived(!!report && !!entries && report.agents.length === 0 && entries.length === 0);
  const hot = $derived(hotAgents(needs));
  const colors = $derived(agentColors(report?.agents ?? []));
  const LAYOUT_ICON: Record<HomeLayout, string> = { cards: "layers", map: "radar", "3d": "box" };
  const layoutOptions = HOME_LAYOUTS.map((l) => ({ value: l, label: HOME_LAYOUT_LABEL[l], icon: LAYOUT_ICON[l], title: `${HOME_LAYOUT_LABEL[l]} · V` }));

  let hovered = $state<string | null>(null);
  let localSelected = $state<string | null>(null);
  const selected = $derived(onselectagent ? selectedAgent : localSelected);
  function setSelected(id: string | null) { if (onselectagent) onselectagent(id); else localSelected = id; }
  const active = $derived(hovered ?? selected);
  const activeNeeds = $derived(active ? new Set(needs.filter((n) => attentionAgent(n, report) === active).map((n) => n.id)) : null);

  /** Skill security at a glance: open findings and the skills they're about (only once an analyzer is on or has run). */
  const skillSec = $derived.by(() => {
    const on = insightsStore.analyzers.some((a) => a.state === "approved");
    const results = insightsStore.insights?.results ?? [];
    if (!on && !results.length) return null;
    const open = results.flatMap((r) => r.findings).filter((f) => f.state === "open");
    return { open: open.length, skills: new Set(open.map((f) => f.subject)).size };
  });
  const stats = $derived<SummaryStat[]>([
    { id: "agents", value: o.agents, label: "agents", go: () => onview("agents") },
    { id: "ports", value: o.ports, label: "listening", go: () => onview("list") },
    { id: "dev", value: o.devServers, label: "dev servers", go: () => onview("list") },
    { id: "exposed", value: o.exposed, label: "exposed", tone: o.exposed ? "warn" : undefined, go: () => onview("list") },
    ...(skillSec ? [{ id: "skill-findings", value: skillSec.open, label: "skill findings", meta: skillSec.skills ? `in ${skillSec.skills} ${skillSec.skills === 1 ? "skill" : "skills"}` : undefined, tone: skillSec.open ? "warn" : "ok", go: () => onsecurity?.() ?? onview("agents") } satisfies SummaryStat] : []),
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

  // The 3D machine and three.js load only when that layout is chosen, and not on a phone-sized
  // window, where Home shows the list of agents instead (as the Map does).
  let Machine3D = $state<typeof import("./machine3d/Machine3DView.svelte").default | null>(null);
  let machine3dError = $state(false);
  let narrow = $state(false);
  $effect(() => {
    const media = matchMedia("(max-width: 760px)");
    narrow = media.matches;
    const change = () => (narrow = media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  });
  $effect(() => {
    if (layout !== "3d" || narrow || Machine3D) return;
    import("./machine3d/Machine3DView.svelte").then((m) => { Machine3D = m.default; machine3dError = false; }, (e) => { console.error("Couldn't load the 3D view", e); machine3dError = true; });
  });
  const spatial = $derived((layout === "map" || layout === "3d") && !idle);
  // 3D Focus: the summary and Needs you step aside so the machine fills Home (its readings carry
  // the same figures, and Needs you is one of them). Remembered; ignored on a narrow window.
  let focus3d = $state(prefGetBoolean("home.3d.focus", false));
  $effect(() => prefSet("home.3d.focus", focus3d ? "1" : "0"));
  const focused = $derived(layout === "3d" && focus3d && !narrow && !idle);
  // Every panel folds away and comes back: the summary, the side column, and each panel in it.
  let summaryOpen = $state(prefGetBoolean("home.summary.open", true));
  $effect(() => prefSet("home.summary.open", summaryOpen ? "1" : "0"));
  let sideOpen = $state(prefGetBoolean("home.side.open", true));
  $effect(() => prefSet("home.side.open", sideOpen ? "1" : "0"));
  const side = $derived(sideOpen && !focused);
  /** The 3D view's readings and selected part, shown above Needs you. */
  let details3d = $state<Snippet | null>(null);
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="home" class:three={layout === "3d" && !idle} id="home-overview" role="region" aria-label="Overview" {onkeydown}>
  <header class="top">
    {#if !focused}
      <IconButton icon={summaryOpen ? "chevron-up" : "chevron-down"} label={summaryOpen ? "Fold the summary" : "Show the summary"} size="sm" aria-expanded={summaryOpen} onclick={() => (summaryOpen = !summaryOpen)} />
      {#if summaryOpen}<HomeSummary {stats} />{:else}<span class="folded">Summary</span>{/if}
    {/if}
    <div class="tools">
      {#if layout === "map"}
        <span class="legend" aria-label="Legend">
          <span><i class="k project"></i>Project</span><span><i class="k port"></i>Port</span><span><i class="k mcp"></i>MCP</span><span><i class="k remote"></i>Remote</span><span><i class="k warn"></i>Needs you</span>
        </span>
      {/if}
      <ExportActions label="Export overview" disabled={!report || !entries} copy={() => homeSummaryText(o, needs, Date.now())}
        files={[{ label: "Download JSON", filename: reportFilename("overview", Date.now(), "json"), build: () => homeReport(o, needs, report, Date.now()) }]} />
      {#if showInventory && oninventory}<ConsentGate onconsent={oninventory} compact />{/if}
      <SegmentedControl value={layout} options={layoutOptions} label="Home layout" onchange={(l) => onlayout?.(l)} />
      {#if !focused}<IconButton icon="sidebar" label={sideOpen ? "Hide the side panels" : "Show the side panels"} size="sm" aria-expanded={sideOpen} onclick={() => (sideOpen = !sideOpen)} />{/if}
    </div>
  </header>

  <div class="body" class:map={spatial} class:three={layout === "3d" && !idle} class:focused={!side}>
    {#key layout}
      <div class="main">
        {#if idle}
          <EmptyPanel icon="bot" title="No agents or dev servers running" hint="Start Claude Code, Cursor, Codex or a dev server and it shows up here within a few seconds." />
        {:else if layout === "cards"}
          {#if report}
            <AgentCards {report} {entries} {needs} {colors} {now} {selected} {active} onhover={(id) => (hovered = id)} onselect={select}
              onopen={(id) => onopenagent(id)} onport={port} onstop={onstopagent} />
          {:else if agentsError}
            <EmptyPanel icon="alert" title="Couldn't look up the agents" hint={agentsError}>
              {#if onretryagents}<Button size="sm" onclick={onretryagents}>Try again</Button>{/if}
            </EmptyPanel>
          {:else}
            <p class="clear wait" aria-busy="true">…</p>
          {/if}
        {:else}
          {#snippet machineMap()}
            <MachineMap {groups} {colors} platform={report?.platform ?? ""} ports={o.ports} {active} {selected} {hot} onhover={(id) => (hovered = id)} onselect={select} onopen={open} onport={port} />
          {/snippet}
          <div class="stage" data-capture-area>
            <div class="map-wrap" class:flush={layout === "3d"}>
              {#if layout === "map"}
                {@render machineMap()}
              {:else if narrow}
                <!-- The list of agents below stands in on a narrow window. -->
              {:else if Machine3D}
                <Machine3D {report} {entries} {hot} needs={needsByAgent(needs, report)} {colors} trust={trustStore.report?.skills ?? []} {selected} {active} {inventory} {singleKeys} {activity} {recording} expanded={focused} onexpand={(on) => (focus3d = on)} {oninventory}
                  onselectagent={setSelected} onhover={(id) => (hovered = id)} onopenagent={(id) => onopenagent(id)} onport={port} ondetails={side ? (d) => (details3d = d) : undefined} fallback={machineMap} />
              {:else if machine3dError}
                <p class="clear">Couldn't load the 3D view. <button type="button" class="inv" onclick={() => onlayout?.("map")}>Show the Map</button></p>
              {:else}
                <p class="clear wait" aria-busy="true">Loading 3D…</p>
              {/if}
            </div>
            <div class="map-list"><MapList {groups} {colors} {active} onhover={(id) => (hovered = id)} onopenagent={(id) => onopenagent(id)} onport={port} /></div>
          </div>
        {/if}
      </div>
    {/key}
    {#if side}
    <!-- The side column: in 3D the machine's readings and selected part, then Needs you. Each panel folds away. -->
    <aside class="side" aria-label="Side panels">
      {#if layout === "3d" && details3d && !idle}
        <CollapsiblePanel id="home-details" title="Details" pref="home.details.open">{@render details3d()}</CollapsiblePanel>
      {/if}
      <CollapsiblePanel id="home-needs" title="Needs you" count={needs.length} pref="home.needs.open" fill>
        {#if trustAlerts.length && onsecurity}
          <button type="button" class="trust-alert" class:changed={trustChanged > 0} onclick={openSkills} data-trust-alert>
            <Icon name="hash" size={13} />
            <span>{#if trustChanged}{trustChanged} skill{trustChanged === 1 ? "" : "s"} changed since you trusted {trustChanged === 1 ? "it" : "them"}{:else}{trustAlerts.length} skill{trustAlerts.length === 1 ? "" : "s"} not trusted yet{/if}</span>
            <span class="go">Review</span>
          </button>
        {/if}
        {#if needs.length}
          <div class="needs-scroll">
            <NeedsList items={needs} lit={activeNeeds} onopen={openItem} onstop={stopItem} onhover={(i) => (hovered = i ? attentionAgent(i, report) : null)} />
          </div>
        {:else if entries && report}
          <EmptyPanel compact tone="ok" icon="check" title="All clear" hint="Nothing needs you right now." />
        {:else}
          <p class="clear" aria-busy="true">…</p>
        {/if}
      </CollapsiblePanel>
    </aside>
    {/if}
  </div>

  {#if layout === "map" && !idle}
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
  /* The 3D machine takes exactly the height Home has left under its header, so the page never
     gains a scrollbar that would resize the canvas while it lays out. */
  .home.three { grid-template-rows: auto minmax(440px, 1fr); align-content: stretch; }
  .body.map.three { height: auto; min-height: 0; }
  .body.three.focused { grid-template-columns: minmax(0, 1fr); }
  .main { min-width: 0; min-height: 0; height: 100%; animation: view-in 180ms ease both; }
  @keyframes view-in { from { opacity: 0; transform: translateY(3px); } }
  @media (prefers-reduced-motion: reduce) { .main { animation: none; } }
  .body:not(.map) .side { position: sticky; top: 0; max-height: calc(100vh - 220px); }
  .wait { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .stage { height: 100%; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; min-height: 0; overflow: clip; }
  .map-wrap { width: 100%; height: 100%; padding: var(--sp-2); }
  .map-wrap.flush { padding: 0; }
  .map-list { display: none; }
  /* The side column: panels stacked; Details keeps its own height (scrolling past 55%), and
     Needs you takes the rest, its list scrolling inside it. A folded panel is just its header. */
  .side { display: flex; flex-direction: column; gap: var(--sp-3); min-height: 0; }
  .side > :global(.cpanel:first-child:not(:last-child)) { flex: 0 1 auto; max-height: 58%; }
  .side > :global(.cpanel:first-child:not(:last-child) > .body) { overflow-y: auto; }
  .side :global(.cpanel > .body > :last-child) { flex: 1 1 auto; min-height: 0; }
  .side :global(.cpanel > .body > .trust-alert) { margin: var(--sp-2) var(--sp-2) 0; width: auto; }
  .needs-scroll { overflow-y: auto; min-height: 0; }
  .folded { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; }
  .clear { margin: 0; padding: var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .clear :global(svg) { color: var(--ok); }
  .lanes-empty { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  @media (max-width: 1100px) { .body { grid-template-columns: minmax(0, 1fr) 280px; } .body.map { height: clamp(300px, 38vh, 480px); } }
  @media (max-width: 860px) {
    .body, .body.map { grid-template-columns: minmax(0, 1fr); height: auto; }
    .body.map .stage { height: 420px; }
    .home.three { grid-template-rows: none; align-content: start; }
    .body.three .stage { height: 560px; }
    .body:not(.map) .side { position: static; }
    .side { order: -1; }
    .side { max-height: none !important; }
  }
  @media (max-width: 760px) {
    .home { padding: var(--sp-3); gap: var(--sp-3); }
    .legend { display: none; }
    .body.map .stage { height: auto; }
    .map-wrap { display: none; }
    .map-list { display: block; }
  }
  .trust-alert { display: flex; align-items: center; gap: 8px; width: 100%; min-height: 32px; margin: 0 0 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); color: var(--text); font: inherit; font-size: var(--fs-caption); text-align: left; cursor: pointer; }
  .trust-alert.changed { border-color: var(--danger-soft); }
  .trust-alert.changed :global(svg) { color: var(--danger); }
  .trust-alert:hover { background: var(--row-hover); }
  .trust-alert:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .trust-alert span:first-of-type { flex: 1; min-width: 0; }
  .trust-alert .go { color: var(--accent); }
</style>
