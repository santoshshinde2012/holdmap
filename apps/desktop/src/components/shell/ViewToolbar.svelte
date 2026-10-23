<script lang="ts">
  // The view switcher and the active view's filters (list), map picture (agents) or search reset.
  // Pure presentation: every value is bound to app state, so the toolbar holds no state of its own.
  import { tick } from "svelte";
  import { ActionDock, Button, FilterChip, IconButton, Select } from "../ui";
  import { linksLabel } from "../../lib/graph";
  import type { Picture } from "../../lib/agents";
  import type { Filters } from "../../lib/format";
  import type { PortStats, Sort } from "../../lib/port-list";

  export type View = "home" | "list" | "graph" | "agents";
  export type AgentSection = "overview" | "flow" | "capabilities" | "insights";

  let {
    view = $bindable(),
    agentSection = $bindable(),
    picture = $bindable(),
    flowSource = $bindable(),
    filters = $bindable(),
    sort = $bindable(),
    sideOpen = $bindable(),
    graphQuery = $bindable(),
    agentQuery = $bindable(),
    graphEdges,
    agentCount,
    securityCount = 0,
    stats,
    filtersActive,
    narrow,
    singleKeys,
    projectReady,
    projectCaption,
    onsocketschange,
    onclearfilters,
  }: {
    view: View;
    agentSection: AgentSection;
    picture: Picture;
    flowSource: "connections" | "calls";
    filters: Filters;
    sort: Sort;
    sideOpen: boolean;
    graphQuery: string;
    agentQuery: string;
    graphEdges: number;
    agentCount: number | null;
    /** Open analyzer findings, shown on the Security tab. */
    securityCount?: number;
    stats: PortStats;
    filtersActive: boolean;
    narrow: boolean;
    singleKeys: boolean;
    projectReady: boolean;
    projectCaption: string;
    /** Listening ↔ all sockets needs a fresh scan. */
    onsocketschange: () => void;
    onclearfilters: () => void;
  } = $props();

  const dock = $derived(narrow ? "compact" : "md");

  // Between "narrow" and a wide window the active view's controls can still outgrow one line (the
  // Agents map pictures, the List filters). Rather than clip them, stack onto a second row.
  let bar = $state<HTMLElement | null>(null);
  let crowded = $state(false);
  async function measure() {
    if (!bar) return;
    crowded = false;
    await tick();
    crowded = !!bar && bar.scrollWidth > bar.clientWidth + 1;
  }
  $effect(() => {
    // Re-measure whenever the controls change (view, section, picture, filters) or the window resizes.
    void [view, agentSection, picture, flowSource, filters, filtersActive, sideOpen, projectCaption, stats, narrow];
    void measure();
  });
  $effect(() => {
    if (!bar || typeof ResizeObserver === "undefined") return;
    let width = bar.clientWidth;
    const ro = new ResizeObserver(() => {
      const next = bar?.clientWidth ?? width;
      if (next !== width) { width = next; void measure(); }
    });
    ro.observe(bar);
    return () => ro.disconnect();
  });
</script>

<nav aria-label="Views">
<div class="toolbar" class:stacked={narrow || crowded} bind:this={bar} role="toolbar" aria-label="View and filters">
  <ActionDock hinge={false} animate={false} label="View" bind:value={view} options={[{ value: "home", label: "Home", icon: "radar", title: "Overview: what is running and what needs a look" }, { value: "list", label: "List", icon: "list", title: singleKeys ? "Port list (G)" : "Port list" }, { value: "graph", label: "Graph", icon: "graph", count: graphEdges || null, countLabel: linksLabel(graphEdges), title: singleKeys ? "Service graph (G)" : "Service graph" }, { value: "agents", label: "Agents", icon: "bot", count: agentCount || null, countLabel: agentCount !== null ? `${agentCount} agents and tools` : undefined, title: singleKeys ? "Agents, tools and apps (⇧A)" : "Agents, tools and apps" }]} />
  <!-- Narrow windows: the active view's controls wrap onto their own rows instead of scrolling sideways. -->
  {#if view !== "home"}
  <div class="row">
    {#if view === "agents"}
      <ActionDock size={dock} hinge={!narrow} label="Agents workspace" bind:value={agentSection} options={[{ value: "overview", label: "Overview", icon: "bot", title: "Agents, resources and access" }, { value: "capabilities", label: "MCP & skills", icon: "plug", title: "MCP servers and skills your agents can load" }, { value: "flow", label: "Flow", icon: "branch", title: "Connections and recorded calls" }, { value: "insights", label: "Security", icon: "shield", count: securityCount || undefined, countLabel: securityCount ? `${securityCount} open findings` : undefined, title: "Security: analyzers and their findings" }]} />
      {#if agentSection === "overview"}
        <ActionDock size={dock} hinge={!narrow} label="Map picture" bind:value={picture} options={[
          { value: "context", label: "Computer", icon: "monitor", title: "Context: this computer, the agents on it, and systems they talk to" },
          { value: "footprint", label: "Footprint", icon: "layers", title: "Containers: folders, tools, ports and hosts across every agent" },
          { value: "project", label: "Project", icon: "folder", title: projectReady ? `Components in ${projectCaption || "the selected source tree"}` : "Select a folder on Footprint to see that source tree", disabled: !projectReady },
        ]} />
      {/if}
      {#if agentSection === "flow"}
        <ActionDock size={dock} hinge={!narrow} label="Flow data source" bind:value={flowSource} options={[{ value: "connections", label: "Connections", icon: "link", title: "Observed process trees, listeners and live sockets" }, { value: "calls", label: "Calls", icon: "activity", title: "Recorded tool calls from configured agents" }]} />
      {/if}
    {/if}
    {#if view === "list"}
      <ActionDock size={dock} hinge={!narrow} label="Socket states" value={filters.all ? "all" : "listen"} options={[{ value: "listen", label: "Listening" }, { value: "all", label: "All sockets", title: `Include established, TIME_WAIT…${singleKeys ? " (A)" : ""}` }]} onchange={(v) => { filters.all = v === "all"; onsocketschange(); }} />
      <ActionDock size={dock} hinge={false} animate={false} label="Protocol" bind:value={filters.proto} options={[{ value: "any", label: "Any" }, { value: "tcp", label: "TCP" }, { value: "udp", label: "UDP" }]} />
      <span class="divider" aria-hidden="true"></span>
      <FilterChip label="Dev servers" dot="var(--tone-green)" count={stats.dev} bind:pressed={filters.dev} title="Only dev servers" kbd={singleKeys ? "D" : undefined} />
      <FilterChip label="Mine" icon="lock" count={stats.mine} bind:pressed={filters.mine} title="Only my processes" kbd={singleKeys ? "M" : undefined} />
      <FilterChip label="Exposed" icon="globe" count={stats.exposed} tone={stats.exposed > 0 ? "warn" : "accent"} bind:pressed={filters.exposed} title="Reachable from the network" kbd={singleKeys ? "E" : undefined} />
      {#if filtersActive}<Button size="sm" variant="ghost" icon="x" onclick={onclearfilters}>Clear</Button>{/if}
      <div class="spacer"></div>
      {#if !narrow}<IconButton icon="panel" size="sm" pressed={sideOpen} label={sideOpen ? "Hide details pane" : "Show details pane"} kbd={singleKeys ? "B" : undefined} onclick={() => (sideOpen = !sideOpen)} />{/if}
      <Select size="sm" prefix="Sort" label="Sort by" labelHidden bind:value={sort} width="168px" options={[{ value: "group", label: "Grouped", description: "By kind: dev, containers, databases…" }, { value: "cluster", label: "Cluster", description: "Compose project, workspace, supervisor" }, { value: "port", label: "Port", description: "Lowest port number first" }, { value: "newest", label: "Newest", description: "Most recently seen first" }, { value: "memory", label: "Memory", description: "Highest memory use first" }]} />
    {:else if view === "graph"}
      <div class="spacer"></div>
      {#if graphQuery}<Button size="sm" variant="ghost" icon="x" onclick={() => (graphQuery = "")}>Clear search</Button>{/if}
    {:else if view === "agents"}
      <div class="spacer"></div>
      {#if agentQuery}<Button size="sm" variant="ghost" icon="x" onclick={() => (agentQuery = "")}>Clear search</Button>{/if}
    {/if}
  </div>
  {/if}
</div>
</nav>

<style>
  .toolbar { display: flex; align-items: center; gap: var(--sp-2); padding: 8px var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); overflow-x: auto; scrollbar-width: none; white-space: nowrap; min-height: 46px; }
  .toolbar::-webkit-scrollbar { display: none; }
  .divider { width: 1px; height: 18px; background: var(--border-strong); margin: 0 var(--sp-1); flex: none; }
  .spacer { flex: 1; min-width: var(--sp-2); }
  .row { display: contents; }
  .toolbar.stacked { flex-wrap: wrap; overflow-x: visible; white-space: normal; row-gap: 6px; }
  .toolbar.stacked .row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px var(--sp-2); flex: 1 1 100%; min-width: 0; }
</style>
