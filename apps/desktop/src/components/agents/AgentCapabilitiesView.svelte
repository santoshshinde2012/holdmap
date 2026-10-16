<script lang="ts">
  import { tick } from "svelte";
  import Icon from "../Icon.svelte";
  import { ActionDock, Button, Callout, Select } from "../ui";
  import CapabilityMap from "./capabilities/CapabilityMap.svelte";
  import CapabilityCatalog from "./capabilities/CapabilityCatalog.svelte";
  import CapabilityComparison from "./capabilities/CapabilityComparison.svelte";
  import CapabilityInspector from "./capabilities/CapabilityInspector.svelte";
  import { agentColor, monogram } from "../../lib/agents";
  import { humanBytes } from "../../lib/format";
  import { isComposingKey } from "../../lib/keyboard";
  import type { AgentsReport, CapabilityScope } from "../../lib/types";
  import { capabilityContextGroups, capabilityItems, capabilityMatches, filterCapabilities, runningMcpItems, type CapabilitySelection, type InventoryKind, type InventoryState } from "../../lib/agent-capabilities";

  let { report, query = "", selectedAgent = null, loading = false, error = null, demo = false, dark = false, reduced = false, onselectagent, onclearquery, onretry, onrecordedcalls }: {
    report: AgentsReport | null; query?: string; selectedAgent: string | null;
    loading?: boolean; error?: string | null; demo?: boolean; dark?: boolean; reduced?: boolean;
    onselectagent: (id: string | null) => void; onclearquery?: () => void;
    onretry?: () => void; onrecordedcalls?: () => void;
  } = $props();
  let kind = $state<InventoryKind | "running" | "tools">("all");
  let view = $state<"explore" | "map" | "compare">("explore");
  let groupedContext = $state<string | null>(null);
  let contextAnchor = $state<string | null>(null);
  let compareAgentIds = $state<string[]>([]);
  let expandedContexts = $state<string[]>([]);
  let inspector: { focusHeading(): void } | undefined = $state();
  let scope = $state<"all" | CapabilityScope>("all");
  let enablement = $state<InventoryState>("all");
  let picked = $state<string | null>(null);
  let inspectingAgent = $state(false);
  let browseOpen = $state(false);
  let filtersOpen = $state(false);
  let coverageOpen = $state(false);
  let canvas: { focusNode(id: string): boolean } | undefined = $state();
  let inspectorHeading: HTMLHeadingElement | undefined = $state();
  let opener: HTMLElement | null = null;
  let returnNode: string | null = null;
  let panelOpener: HTMLElement | null = null;
  let coverageHeading: HTMLHeadingElement | undefined = $state();
  let filterPanel: HTMLDivElement | undefined = $state();
  let workspaceElement: HTMLElement | undefined = $state();
  let railElement: HTMLElement | undefined = $state();
  let detailsElement: HTMLElement | undefined = $state();
  let coveragePanel: HTMLElement | undefined = $state();
  let workspaceWidth = $state(0);
  const overlayRail = $derived(workspaceWidth > 0 && workspaceWidth <= 960);
  const overlayDetails = $derived(workspaceWidth > 0 && workspaceWidth <= 1180);
  const agents = $derived(report?.agents ?? []);
  const contexts = $derived(capabilityContextGroups(agents));
  const context = $derived(groupedContext ? contexts.find(group => group.id === groupedContext) ?? contexts.find(group => group.agents.some(agent => agent.id === contextAnchor)) ?? null : null);
  const active = $derived(context ? null : agents.find((agent) => agent.id === selectedAgent) ?? null);
  const scoped = $derived(view === "compare" ? agents.filter(agent => compareAgentIds.includes(agent.id)) : context?.agents ?? (active ? [active] : agents));
  const mapContexts = $derived(capabilityContextGroups(scoped));
  const inventory = $derived(capabilityItems(scoped));
  const processes = $derived(runningMcpItems(scoped));
  const visible = $derived(kind === "running" || kind === "tools" ? [] : filterCapabilities(inventory, query, kind, scope, enablement));
  const running = $derived(kind === "all" || kind === "running" ? processes.filter((item) => capabilityMatches(item, query)) : []);
  const chosen = $derived<CapabilitySelection | null>(visible.find((item) => item.id === picked) ?? running.find((item) => item.id === picked) ?? null);
  const showAgent = $derived(inspectingAgent && !!active);
  const showDetails = $derived(!!chosen || showAgent);
  const inspected = $derived(scoped.some((agent) => agent.capabilities?.inspected));
  const mcpCount = $derived(inventory.filter((item) => item.kind === "mcp").length);
  const skillCount = $derived(inventory.filter((item) => item.kind === "skill").length);
  const filtered = $derived(kind !== "all" || scope !== "all" || enablement !== "all");
  const narrowed = $derived(filtered || !!query.trim());
  const agentIds = $derived(agents.map((agent) => agent.id));
  const coverage = $derived.by(() => {
    const groups = new Map<string, { text: string; owners: string[] }>();
    const add = (text: string, owner?: string) => {
      const group = groups.get(text) ?? { text, owners: [] };
      if (owner && !group.owners.includes(owner)) group.owners.push(owner);
      groups.set(text, group);
    };
    for (const agent of scoped) {
      const owner = agent.name + " · PID " + agent.pid;
      const data = agent.capabilities;
      if (!data) add("Capability inventory was not reported by this backend.", owner);
      else {
        if (!data.inspected) add("Configuration and skill sources have not been inspected.", owner);
        for (const limit of data.limits) add(limit, owner);
        if (data.more_mcp_servers) add(data.more_mcp_servers + " more MCP definitions not listed.", owner);
        if (data.more_skills) add(data.more_skills + " more skill files not listed.", owner);
      }
    }
    for (const limit of report?.limits ?? []) add(limit);
    return [...groups.values()];
  });
  $effect(() => {
    if (groupedContext && !context) {
      groupedContext = null; contextAnchor = null;
      void clearDetails(staleFocusNeedsRestoring());
    } else if (context) {
      if (groupedContext !== context.id) groupedContext = context.id;
      if (!context.agents.some(agent => agent.id === contextAnchor)) contextAnchor = context.representative.id;
    }
  });
  $effect(() => { if (picked && !chosen) void clearDetails(staleFocusNeedsRestoring()); });
  $effect(() => { if (inspectingAgent && !active) void clearDetails(staleFocusNeedsRestoring()); });

  function staleFocusNeedsRestoring() {
    const focused = document.activeElement;
    return !focused || focused === document.body || !!workspaceElement?.querySelector('[aria-label="Capability details"]')?.contains(focused);
  }

  function focusBrowser() {
    const rail = workspaceElement?.querySelector<HTMLElement>(".rail");
    const target = rail && getComputedStyle(rail).display !== "none"
      ? rail.querySelector<HTMLElement>(".agent-choice.all")
      : workspaceElement?.querySelector<HTMLElement>(".browse-button");
    target?.focus({ preventScroll: true });
  }

  function remember(id: string) {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    returnNode = id;
  }
  async function select(id: string) {
    remember(id);
    picked = id;
    inspectingAgent = false;
    browseOpen = false;
    await tick();
    inspector?.focusHeading();
  }
  async function selectAgent(id: string | null) {
    groupedContext = null; contextAnchor = null;
    if (id) remember(id);
    picked = null;
    inspectingAgent = !!id;
    browseOpen = false;
    onselectagent(id);
    await tick();
    if (id) inspectorHeading?.focus({ preventScroll: true });
    else focusBrowser();
  }
  async function selectContext(id: string) {
    groupedContext = id;
    contextAnchor = contexts.find(group => group.id === id)?.representative.id ?? null;
    picked = null; inspectingAgent = false; browseOpen = false;
    onselectagent(null);
    await tick();
    if (overlayRail) focusBrowser();
  }
  function selectMapAgent(id: string | null) {
    const group = mapContexts.find(group => group.representative.id === id);
    if (group && group.agents.length > 1) void selectContext(group.id);
    else void selectAgent(id);
  }
  function toggleProcesses(id: string) { expandedContexts = expandedContexts.includes(id) ? expandedContexts.filter(value => value !== id) : [...expandedContexts, id]; }
  function changeView() { browseOpen = false; }
  function compareContexts(ids: string[]) {
    if (compareAgentIds.join("\0") === ids.join("\0")) return;
    compareAgentIds = ids;
    if (view === "compare") void clearDetails(false);
  }
  function closeDetails() { return clearDetails(true); }
  async function clearDetails(restoreFocus: boolean) {
    const id = returnNode;
    picked = null;
    inspectingAgent = false;
    browseOpen = false;
    await tick();
    if (!restoreFocus) return;
    const focusVisible = (element: HTMLElement | null) => {
      if (!element?.isConnected || element.closest("[hidden]")) return false;
      element.focus({ preventScroll: true });
      return document.activeElement === element;
    };
    if (focusVisible(opener)) return;
    const cards = [...workspaceElement?.querySelectorAll<HTMLButtonElement>("button[data-capability-id]") ?? []].filter(button => button.dataset.capabilityId === id);
    for (const card of cards) if (focusVisible(card)) return;
    if (!id || !canvas?.focusNode(id)) focusBrowser();
  }
  function reset() { kind = "all"; scope = "all"; enablement = "all"; picked = null; }
  function changeKind() { enablement = "all"; if (kind === "running" || kind === "tools") scope = "all"; picked = null; }
  async function closePanels() {
    coverageOpen = false;
    filtersOpen = false;
    await tick();
    if (panelOpener?.isConnected) panelOpener.focus({ preventScroll: true });
  }
  function contains(element: HTMLElement | undefined | null, target: EventTarget | null) {
    return target instanceof Node && !!element?.contains(target);
  }
  function leavePanel(event: FocusEvent) {
    if (!coverageOpen && !filtersOpen) return;
    if (contains(event.currentTarget as HTMLElement, event.relatedTarget) || contains(panelOpener, event.relatedTarget)) return;
    // These are nonmodal disclosures: leaving them must reveal the newly focused
    // control, rather than returning focus to an unrelated opener.
    coverageOpen = false;
    filtersOpen = false;
    if (overlayDetails && !contains(detailsElement, event.relatedTarget) && showDetails) void clearDetails(false);
  }
  function leaveRail(event: FocusEvent) {
    if (!event.relatedTarget || !overlayRail || contains(railElement, event.relatedTarget) || contains(coveragePanel, event.relatedTarget) || contains(filterPanel, event.relatedTarget)) return;
    browseOpen = false;
  }
  function leaveDetails(event: FocusEvent) {
    if (!event.relatedTarget || !overlayDetails || contains(detailsElement, event.relatedTarget) || contains(coveragePanel, event.relatedTarget) || contains(filterPanel, event.relatedTarget) || contains(opener, event.relatedTarget)) return;
    if (showDetails) void clearDetails(false);
  }
  function outsidePointer(event: PointerEvent) {
    const target = event.target;
    if (!contains(panelOpener, target)) {
      if (coverageOpen && !contains(coveragePanel, target)) coverageOpen = false;
      if (filtersOpen && !contains(filterPanel, target)) filtersOpen = false;
    }
    if (overlayRail && browseOpen && !contains(railElement, target) && !contains(opener, target)) browseOpen = false;
    if (overlayDetails && showDetails && !contains(detailsElement, target) && !contains(coveragePanel, target) && !contains(filterPanel, target) && !contains(opener, target)) void clearDetails(false);
  }
  async function openPanel(panel: "coverage" | "filters") {
    if ((panel === "coverage" && coverageOpen) || (panel === "filters" && filtersOpen)) {
      await closePanels();
      return;
    }
    panelOpener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    coverageOpen = panel === "coverage";
    filtersOpen = panel === "filters";
    await tick();
    if (coverageOpen) coverageHeading?.focus({ preventScroll: true });
    else filterPanel?.querySelector<HTMLElement>('[role="combobox"]')?.focus({ preventScroll: true });
  }
  async function browseAgents() {
    if (browseOpen) { void closeDetails(); return; }
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    returnNode = null;
    browseOpen = true;
    picked = null;
    inspectingAgent = false;
    await tick();
    workspaceElement?.querySelector<HTMLElement>(".agent-choice.all")?.focus({ preventScroll: true });
  }
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || isComposingKey(event)) return;
    if (coverageOpen || filtersOpen) { event.preventDefault(); event.stopPropagation(); void closePanels(); return; }
    if (showDetails || browseOpen) { event.stopPropagation(); event.preventDefault(); void closeDetails(); }
  }
</script>

<svelte:window onpointerdown={outsidePointer} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (Dismiss local panels after child controls handle their own Escape keys.) -->
<section id="agent-capabilities" class="capabilities" aria-label="MCP and skills workspace" aria-busy={loading} bind:this={workspaceElement} bind:clientWidth={workspaceWidth} {onkeydown}>
  <header class="workspace-head">
    <div class="workspace-title"><span class="title-icon"><Icon name="plug" size={19} /></span><div><h2>MCP &amp; skills {#if demo}<span class="preview">Example inventory</span>{/if}</h2><p>Understand purpose, compare sources and inspect the evidence around your agents.</p></div></div>
    <div class="head-actions">
      <Button size="sm" variant={coverageOpen ? "soft" : "ghost"} icon="info" aria-expanded={coverageOpen} aria-controls="capability-coverage" onclick={() => openPanel("coverage")}>Coverage</Button>
      {#if onrecordedcalls}<Button size="sm" icon="activity" onclick={onrecordedcalls}>Recorded calls</Button>{/if}
    </div>
  </header>
  {#if error}<div class="notice" role="alert"><Callout tone="warn" title="Couldn't refresh capability inventory">{error}</Callout>{#if onretry}<Button size="sm" icon="refresh" onclick={onretry}>Try again</Button>{/if}</div>{/if}
  {#if !report}
    <div class="empty" role="status"><Icon name="plug" size={28} /><h3>{error ? "Inventory unavailable" : "Loading agents…"}</h3><p>Inspecting supported local capability sources.</p></div>
  {:else if !agents.length}
    <div class="empty"><Icon name="bot" size={30} /><h3>No running agents to inspect</h3><p>Start a supported agent, then refresh. This map describes recognized running agents; it is not a list of everything installed on this computer.</p>{#if onretry}<Button size="sm" icon="refresh" onclick={onretry}>Refresh agents</Button>{/if}</div>
  {:else}
    <div class="workspace" class:has-details={showDetails}>
      <aside class="rail" class:is-open={browseOpen} aria-label="Capability agents" bind:this={railElement} onfocusout={leaveRail}>
        <header class="rail-head"><h3>Contexts <span>{contexts.length}</span></h3>{#if browseOpen}<Button variant="ghost" size="xs" icon="x" aria-label="Close agent browser" onclick={closeDetails} />{:else}<span class="rail-hint">{agents.length} processes</span>{/if}</header>
        <nav class="agent-list" aria-label="Capability agents">
          <button class="agent-choice all" class:on={!active && !context} aria-pressed={!active && !context} onclick={() => selectAgent(null)}><span class="agent-avatar" aria-hidden="true"><Icon name="layers" size={17} /></span><span><b>All agents</b><small>Shared definitions shown once</small></span><Icon name="chevron" size={13} /></button>
          {#each contexts as group (group.id)}
            <div class="context-choice">
              <button class="agent-choice" class:on={context?.id === group.id || !!active && group.agents.some(agent => agent.id === active.id)} aria-pressed={context?.id === group.id || !!active && group.agents.some(agent => agent.id === active.id)} aria-label={`Explore context ${group.name}, ${group.agents.length} observed processes`} onclick={() => selectContext(group.id)}>
                <span class="agent-avatar" aria-hidden="true" style={"--agent: " + agentColor(null, group.representative.id, agentIds)}>{monogram(group.name)}</span><span><b>{group.name}</b><small>{group.agents.length} observed {group.agents.length === 1 ? "process" : "processes"}</small><small class="context-project">{group.projectPaths.join(" · ") || "No project reported"}</small></span><Icon name="chevron" size={13} />
              </button>
              <button class="process-toggle" aria-label={`Show processes for ${group.name}`} aria-expanded={expandedContexts.includes(group.id)} onclick={() => toggleProcesses(group.id)}>Process details <Icon name={expandedContexts.includes(group.id) ? "chevron-up" : "chevron-down"} size={12} /></button>
              {#if expandedContexts.includes(group.id)}<div class="process-list">{#each group.agents as agent (agent.id)}<button class="process-choice" aria-pressed={active?.id === agent.id} aria-label={"Inspect agent card " + agent.name + ", PID " + agent.pid} onclick={() => selectAgent(agent.id)}>{agent.name} <code>PID {agent.pid}</code></button>{/each}</div>{/if}
            </div>
          {/each}
        </nav>
        <p class="rail-note">Matching source observations are grouped. A process group is not a shared session or proof of loaded capabilities.</p>
      </aside>
      <div class="map-area">
        <div class="map-tools">
          <div class="map-context"><button class="browse-button" aria-label="Browse agents" aria-expanded={browseOpen} onclick={browseAgents}><Icon name="sidebar" size={15} /></button><span><b>{view === "compare" ? "Compare contexts" : context?.name ?? active?.name ?? "All agents"}</b><small>{loading ? "Inspecting local sources…" : view === "compare" ? "Exact observed definitions · coverage varies" : context ? context.agents.length + " observed processes · matching source inventory" : active ? "PID " + active.pid : contexts.length + " source contexts · " + agents.length + " observed processes"}</small></span>{#if active || context}<button class="all-link" onclick={() => selectAgent(null)}>Show all <Icon name="arrow" size={12} /></button>{/if}</div>
          <div class="view-controls"><ActionDock size="compact" hinge={false} animate={false} label="Capability view" bind:value={view} onchange={changeView} options={[{ value: "explore", label: "Explore", icon: "layers" }, { value: "map", label: "Map", icon: "graph" }, { value: "compare", label: "Compare", icon: "panel" }]} /></div>
          <div class="map-filters"><ActionDock size="compact" hinge={false} animate={false} label="Capability type" bind:value={kind} onchange={changeKind} options={[{ value: "all", label: "All" }, { value: "mcp", label: "MCP servers", icon: "plug", count: inspected ? mcpCount : undefined }, { value: "skill", label: "Skills", icon: "sparkles", count: inspected ? skillCount : undefined }, { value: "tools", label: "Tools", icon: "terminal" }, { value: "running", label: "Running", icon: "cpu", count: processes.length }]} /><Button size="xs" variant={filtersOpen || scope !== "all" || enablement !== "all" ? "soft" : "ghost"} icon="filter" aria-expanded={filtersOpen} aria-controls="capability-filters" onclick={() => openPanel("filters")}>Filters</Button></div>
          {#if filtersOpen}<div class="filter-panel" id="capability-filters" bind:this={filterPanel} onfocusout={leavePanel}><Select label="Capability scope" prefix="Scope" labelHidden size="sm" disabled={kind === "running" || kind === "tools"} bind:value={scope} options={[{ value: "all", label: "All scopes" }, { value: "user", label: "User" }, { value: "project", label: "Project" }, { value: "local", label: "Local project" }]} /><Select label="MCP enablement" prefix="State" labelHidden size="sm" bind:value={enablement} disabled={kind === "skill" || kind === "running" || kind === "tools"} options={[{ value: "all", label: "All states" }, { value: "enabled", label: "Enabled MCP" }, { value: "disabled", label: "Disabled MCP" }, { value: "unknown", label: "Unknown MCP" }]} />{#if filtered}<Button size="xs" variant="ghost" onclick={reset}>Reset filters</Button>{/if}</div>{/if}
        </div>
        <div class="result-summary" role="status">{kind === "tools" ? "Tool catalog not reported by inventory" : view === "compare" ? "Compare supported source observations" : `${visible.length} of ${inventory.length} distinct definitions reported · ${running.length} of ${processes.length} runtime candidates`}</div>
        <div class="canvas-host" class:scrollable={view !== "map" || kind === "tools"}>
          <div hidden={view !== "compare" || kind === "tools"}>
            <CapabilityComparison groups={contexts} {query} {kind} {scope} {enablement} selectedId={chosen?.id ?? null} onselect={select} oncontextchange={compareContexts} />
          </div>
          {#if kind === "tools"}
            <div class="tools-empty"><span class="empty-icon"><Icon name="terminal" size={27} /></span><h3>Catalog not reported</h3><p>Configuration names do not enumerate an agent’s individual tools. This inventory does not report MCP tool catalogs or built-in actions. Unknown does not mean zero tools.</p><p>Use the agent client’s tool settings to inspect its catalog. Optional recorded calls provide separate evidence of reported use.</p><div class="empty-actions"><button onclick={() => { kind = "mcp"; enablement = "all"; }}>Explore MCP definitions</button><button onclick={() => openPanel("coverage")}>View coverage</button>{#if onrecordedcalls}<button onclick={onrecordedcalls}>Open recorded calls</button>{/if}</div></div>
          {:else if view !== "compare"}
            {#if view === "map"}<CapabilityMap bind:this={canvas} agents={scoped} contextGroups={mapContexts} items={visible} {running} {agentIds} selectedAgent={context?.representative.id ?? selectedAgent} selectedId={chosen?.id ?? (showAgent ? active?.id ?? null : null)} filtered={narrowed} {dark} {reduced} onselect={select} onselectagent={selectMapAgent} />
            {:else}<CapabilityCatalog items={visible} {running} selectedId={chosen?.id ?? null} onselect={select} />{/if}
            {#if !visible.length && !running.length}
              <div class="map-empty" class:catalog-empty={view === "explore"} role="status"><span class="empty-icon"><Icon name={loading ? "refresh" : narrowed ? "search" : "plug"} size={21} /></span><div><h3>{loading ? "Inspecting capability sources…" : kind === "running" ? query.trim() ? "No runtime matches" : "No runtime candidates" : narrowed ? "No inventory matches" : inspected ? "No entries reported" : "Inventory not inspected"}</h3><p>{kind === "running" ? "No MCP candidate processes were reported for this view. Remote or idle servers may have no local process." : narrowed ? "Try another name, purpose, source path or scope." : "Only supported, accessible sources are shown. Missing entries do not prove an agent has no capabilities."}</p><div class="empty-actions">{#if filtered}<button onclick={reset}>Reset filters</button>{/if}{#if query.trim() && onclearquery}<button onclick={onclearquery}>Clear search</button>{/if}{#if active || context}<button onclick={() => selectAgent(null)}>View all agents</button>{/if}<button onclick={() => openPanel("coverage")}>View coverage</button>{#if kind !== "running" && !inspected && !loading && onretry}<button onclick={onretry}>Inspect sources</button>{/if}</div></div></div>
            {/if}
          {/if}
        </div>
        <div class="evidence-footer"><Icon name="info" size={13} /><span>Found files and definitions are source observations. Loading, approval and use remain separate.</span><button onclick={() => openPanel("coverage")}>Skills, MCP &amp; tools</button></div>
      </div>
      {#if showDetails}
        <aside class="details" aria-label="Capability details" bind:this={detailsElement} onfocusout={leaveDetails}>
          <header class="rail-head"><button class="back" onclick={closeDetails}><Icon name="arrow-left" size={14} />Back to capabilities</button><Button variant="ghost" size="xs" icon="x" aria-label="Close capability details" onclick={closeDetails} /></header>
          {#if chosen}
            {#key chosen.id}<CapabilityInspector bind:this={inspector} item={chosen} takenAt={report.taken_at_ms} onselectagent={selectAgent} {onrecordedcalls} oncoverage={() => openPanel("coverage")} />{/key}
          {:else if active}
            <div class="inspector"><span class="eyebrow">Observed agent process</span><h3 class="inspector-title" tabindex="-1" bind:this={inspectorHeading}>{active.name}</h3><p class="detail-intro">PID {active.pid} · {active.vendor}</p>
            <div class="agent-counts"><span><Icon name="plug" size={15} /><b>{active.capabilities?.inspected ? active.capabilities.mcp_servers.length : "—"}</b>MCP definitions</span><span><Icon name="sparkles" size={15} /><b>{active.capabilities?.inspected ? active.capabilities.skills.length : "—"}</b>Skill files</span></div>
            <p class="agent-hint">Select a capability to inspect its purpose, source and evidence.</p><dl><div><dt>Inventory</dt><dd>{active.capabilities?.inspected ? "Supported sources inspected" : "Inventory not inspected"}</dd></div><div><dt>Resources</dt><dd>{active.cpu_percent.toFixed(1)}% CPU · {humanBytes(active.memory_bytes)}</dd></div></dl>
            <div class="usage-note"><Icon name="info" size={15} /><p>Source observations do not establish loading, approval or use by this process. A PID is not a session identity.</p></div><Button size="sm" variant="ghost" icon="info" onclick={() => openPanel("coverage")}>View coverage</Button></div>
          {/if}
        </aside>
      {/if}
    </div>
  {/if}
  {#if coverageOpen}
    <section class="coverage-panel" id="capability-coverage" aria-label="Visibility limits" bind:this={coveragePanel} onfocusout={leavePanel}>
      <header><div><span class="eyebrow">Local discovery</span><h3 tabindex="-1" bind:this={coverageHeading}>Coverage &amp; sources</h3></div><Button size="xs" variant="ghost" icon="x" aria-label="Close coverage" onclick={closePanels} /></header>
      <p>Configuration and skill files show what is present. Recorded calls provide separate evidence of use.</p>
      <dl class="glossary"><div><dt>Skill</dt><dd>A reusable instruction package. A found SKILL.md file does not establish activation.</dd></div><div><dt>MCP</dt><dd>A protocol through which servers can provide tools, resources and prompts. A definition does not establish a connection.</dd></div><div><dt>Tool</dt><dd>A callable action supplied by an agent host or MCP server. Individual tool catalogs are not reported by this inventory.</dd></div></dl>
      <div class="coverage-stat"><Icon name="code" size={15} /><span>{scoped.filter((agent) => agent.capabilities?.inspected).length} of {scoped.length} agent inventories inspected</span></div>
      {#if coverage.length}<ul>{#each coverage as item}<li><p>{item.text}</p>{#if item.owners.length}<small>{item.owners.length === 1 ? item.owners[0] : "Applies to " + item.owners.length + " agents"}</small>{/if}</li>{/each}</ul>{:else}<p>No additional visibility notes were reported.</p>{/if}
    </section>
  {/if}
</section>

<style>
  .capabilities { position: relative; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--bg); color: var(--text); container-type: inline-size; }
  h2, h3, p { margin: 0; } h2 { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; font-size: var(--fs-heading); line-height: var(--lh-heading); } h3 { font-size: var(--fs-body); line-height: var(--lh-body); }
  button { font: inherit; cursor: pointer; } button:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .workspace-head { display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 14px 20px; border-bottom: 1px solid var(--border); background: var(--surface); flex: none; }
  .workspace-title { display: flex; align-items: center; gap: 11px; min-width: 0; } .title-icon { width: 38px; height: 38px; flex: none; border-radius: 12px; display: grid; place-items: center; color: var(--accent); background: var(--accent-soft); }
  .workspace-title p { color: var(--muted); margin-top: 3px; font-size: var(--fs-caption); line-height: var(--lh-caption); } .head-actions { display: flex; gap: 6px; flex: none; }
  .preview { color: var(--tone-blue); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-full); padding: 2px 7px; }
  .workspace { display: grid; grid-template-columns: 250px minmax(0, 1fr); flex: 1; min-height: 0; position: relative; }
  .workspace.has-details { grid-template-columns: 240px minmax(0, 1fr) 320px; }
  .rail, .details { background: var(--surface); min-width: 0; overflow: auto; overscroll-behavior: contain; } .rail { border-right: 1px solid var(--border); } .details { border-left: 1px solid var(--border); }
  .rail-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 53px; padding: 10px 15px; border-bottom: 1px solid var(--border); } .rail-head h3 { display: flex; gap: 8px; align-items: center; } .rail-head h3 span, .rail-hint { color: var(--muted); font-weight: var(--fw-medium); font-size: var(--fs-caption); }
  .agent-list { display: grid; gap: 10px; padding: 12px; } .agent-choice { display: flex; width: 100%; align-items: center; gap: 10px; border: 1px solid var(--border); border-radius: 12px; padding: 12px; background: var(--surface); color: var(--text-2); text-align: left; box-shadow: var(--shadow-sm); }
  .agent-choice:hover { background: var(--row-hover); border-color: var(--border-strong); } .agent-choice.on { background: var(--row-selected); border-color: var(--accent); }
  .agent-choice > span:nth-child(2) { display: grid; gap: 4px; flex: 1; min-width: 0; } .agent-choice b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  .agent-choice small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .agent-choice > :global(svg) { color: var(--muted); flex: none; } .context-project { overflow-wrap: anywhere; }
  .agent-avatar { --agent: var(--accent); display: grid; place-items: center; flex: none; width: 34px; height: 34px; color: var(--agent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); border-radius: 10px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); }
  .context-choice { border: 1px solid var(--border); border-radius: 12px; overflow: clip; } .context-choice .agent-choice { border-radius: 11px 11px 0 0; border-width: 0 0 1px; box-shadow: none; } .context-choice .agent-choice.on { box-shadow: inset 0 0 0 1px var(--accent); }
  .process-toggle { display: flex; justify-content: space-between; align-items: center; width: 100%; gap: 8px; background: var(--surface-2); color: var(--muted); padding: 8px 12px; border: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .process-list { padding: 5px; border-top: 1px solid var(--border); display: grid; gap: 3px; } .process-choice { display: flex; gap: 8px; align-items: center; justify-content: space-between; width: 100%; padding: 8px 5px; background: var(--surface); color: var(--text-2); border: 0; border-radius: 5px; font-size: var(--fs-caption); line-height: var(--lh-caption); } .process-choice:hover { background: var(--row-hover); } .process-choice[aria-pressed="true"] { background: var(--row-selected); } .process-choice code { color: var(--muted); }
  .rail-note { padding: 5px 18px 20px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .back { display: flex; align-items: center; gap: 6px; border: 0; padding: 4px 0; color: var(--text-2); background: transparent; font-size: var(--fs-body-sm); }
  .map-area { display: flex; flex-direction: column; min-width: 0; min-height: 0; } .map-tools { position: relative; z-index: 3; display: flex; align-items: center; flex-wrap: wrap; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid var(--border); background: var(--surface); flex: none; }
  .map-context { display: flex; align-items: center; gap: 9px; min-width: 0; } .map-context > span { display: grid; gap: 3px; } .map-context b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .map-context small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .browse-button { display: none; border: 1px solid var(--border); border-radius: var(--r-sm); width: 30px; height: 30px; flex: none; place-items: center; color: var(--text-2); background: var(--surface); }
  .all-link { display: flex; gap: 4px; align-items: center; margin-left: 8px; border: 0; background: transparent; color: var(--accent); font-size: var(--fs-caption); }
  .map-filters { display: flex; align-items: center; justify-content: space-between; gap: 7px; flex-wrap: wrap; min-width: 0; width: 100%; }
  .map-filters :global(.dock) { flex-wrap: wrap; flex: 1; min-width: 0; }
  .filter-panel { position: absolute; top: calc(100% + 7px); right: 16px; display: flex; gap: 9px; align-items: center; flex-wrap: wrap; max-width: calc(100% - 32px); padding: 14px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); box-shadow: var(--shadow-lg); } .filter-panel :global(.field) { min-width: 0; }
  .result-summary { padding: 10px 18px; border-bottom: 1px solid var(--border); color: var(--muted); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .canvas-host { flex: 1; min-height: 0; position: relative; } .canvas-host.scrollable { overflow: auto; overscroll-behavior: contain; }
  .evidence-footer { display: flex; align-items: flex-start; gap: 7px; padding: 10px 16px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); border-top: 1px solid var(--border); background: var(--surface); } .evidence-footer :global(svg) { margin-top: 1px; } .evidence-footer span { flex: 1; } .evidence-footer button { background: transparent; border: 0; padding: 0; color: var(--accent); font-size: var(--fs-caption); line-height: var(--lh-caption); text-align: left; }
  .map-empty { position: absolute; z-index: 2; bottom: 60px; left: 50%; transform: translateX(-50%); display: flex; gap: 12px; width: min(430px, calc(100% - 40px)); padding: 16px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); box-shadow: var(--shadow-sm); }
  .map-empty.catalog-empty { position: static; transform: none; margin: 22px auto; } .empty-icon { color: var(--muted); flex: none; padding-top: 2px; }
  .map-empty p, .tools-empty p { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 9px; }
  .empty-actions { display: flex; flex-wrap: wrap; gap: 8px 14px; margin-top: 14px; } .empty-actions button { border: 0; padding: 0; background: transparent; color: var(--accent); font-size: var(--fs-caption); line-height: var(--lh-caption); text-decoration: underline; text-underline-offset: 3px; }
  .tools-empty { margin: 24px; padding: 25px; max-width: 650px; background: var(--surface); border: 1px solid var(--border); border-radius: 15px; } .tools-empty h3 { margin-top: 12px; }
  .coverage-panel { position: absolute; z-index: 8; top: 64px; right: 16px; width: min(420px, calc(100% - 32px)); max-height: calc(100% - 80px); overflow: auto; padding: 20px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); box-shadow: var(--shadow-lg); }
  .coverage-panel header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; } .coverage-panel > p { margin-top: 12px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .coverage-stat { display: flex; gap: 8px; align-items: center; margin-top: 16px; padding: 10px; border-radius: 9px; color: var(--text-2); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .coverage-panel ul { display: grid; gap: 12px; list-style: none; padding: 0; margin: 16px 0 0; } .coverage-panel li { padding-top: 12px; border-top: 1px solid var(--border); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); } .coverage-panel small { display: block; color: var(--muted); margin-top: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .inspector { padding: 22px 18px; } .eyebrow { display: block; color: var(--muted); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; margin-bottom: 8px; } .inspector-title { font-size: var(--fs-heading); line-height: var(--lh-heading); overflow-wrap: anywhere; outline: none; }
  .detail-intro, .agent-hint { margin-top: 12px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .agent-counts { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 20px; } .agent-counts > span { display: grid; grid-template-columns: auto 1fr; gap: 6px; padding: 12px; background: var(--surface-2); border-radius: 10px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .agent-counts b { color: var(--text); font-size: var(--fs-heading); }
  dl { display: grid; gap: 17px; margin: 24px 0; } dl > div { display: grid; gap: 5px; } dt { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } dd { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  .usage-note { display: flex; gap: 8px; align-items: flex-start; border-top: 1px solid var(--border); padding-top: 16px; margin-bottom: 12px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .notice { display: flex; align-items: center; gap: 8px; padding: 8px 16px; } .empty { flex: 1; display: flex; flex-direction: column; gap: 12px; align-items: center; justify-content: center; text-align: center; padding: 24px; color: var(--muted); } .empty h3 { color: var(--text-2); } .empty p { max-width: 480px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  @container (max-width: 1180px) { .workspace.has-details { grid-template-columns: 235px minmax(0, 1fr); } .details { position: absolute; z-index: 6; top: 10px; right: 10px; bottom: 10px; width: min(350px, calc(100% - 20px)); border: 1px solid var(--border); border-radius: 14px; box-shadow: var(--shadow-lg); } .workspace-head { padding-inline: 14px; } .workspace-title p { display: none; } }
  @container (max-width: 960px) { .workspace-head { padding: 10px 12px; flex-wrap: wrap; gap: 8px; } .workspace-title { gap: 8px; } .title-icon { width: 30px; height: 30px; border-radius: 9px; } .head-actions { margin-left: auto; } .workspace, .workspace.has-details { grid-template-columns: minmax(0, 1fr); } .rail { display: none; } .rail.is-open { display: block; position: absolute; z-index: 6; top: 10px; left: 10px; bottom: 10px; width: min(330px, calc(100% - 20px)); border: 1px solid var(--border); border-radius: 14px; box-shadow: var(--shadow-lg); } .browse-button { display: grid; } .map-tools { padding: 10px 12px; gap: 9px; } .view-controls { margin-left: auto; } .map-empty { bottom: 60px; width: calc(100% - 28px); padding: 12px; } .coverage-panel { top: 96px; max-height: calc(100% - 110px); } .tools-empty { margin: 15px; padding: 20px; } }
  @container (max-width: 560px) { .map-context { width: 100%; } .view-controls { margin-left: 0; } .result-summary { padding-inline: 12px; } .evidence-footer { padding: 8px 12px; } }
</style>
