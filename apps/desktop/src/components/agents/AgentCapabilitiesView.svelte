<script lang="ts">
  import { tick } from "svelte";
  import Icon from "../Icon.svelte";
  import { ActionDock, Button, Callout, Select } from "../ui";
  import CapabilityMap from "./capabilities/CapabilityMap.svelte";
  import { agentColor, EVIDENCE_LABEL, monogram } from "../../lib/agents";
  import { humanBytes } from "../../lib/format";
  import { isComposingKey } from "../../lib/keyboard";
  import type { AgentMcpServer, AgentSkill, AgentsReport, CapabilityScope } from "../../lib/types";
  import { capabilityItems, capabilityMatches, enabledLabel, filterCapabilities, runningMcpItems, scopeLabel, type CapabilitySelection, type InventoryKind, type InventoryState } from "../../lib/agent-capabilities";

  let { report, query = "", selectedAgent = null, loading = false, error = null, demo = false, dark = false, reduced = false, onselectagent, onclearquery, onretry, onrecordedcalls }: {
    report: AgentsReport | null; query?: string; selectedAgent: string | null;
    loading?: boolean; error?: string | null; demo?: boolean; dark?: boolean; reduced?: boolean;
    onselectagent: (id: string | null) => void; onclearquery?: () => void;
    onretry?: () => void; onrecordedcalls?: () => void;
  } = $props();
  let kind = $state<InventoryKind | "running">("all");
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
  const agents = $derived(report?.agents ?? []);
  const active = $derived(agents.find((agent) => agent.id === selectedAgent) ?? null);
  const scoped = $derived(active ? [active] : agents);
  const inventory = $derived(capabilityItems(scoped));
  const processes = $derived(runningMcpItems(scoped));
  const visible = $derived(kind === "running" ? [] : filterCapabilities(inventory, query, kind, scope, enablement));
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
    inspectorHeading?.focus({ preventScroll: true });
  }
  async function selectAgent(id: string | null) {
    if (id) remember(id);
    picked = null;
    inspectingAgent = !!id;
    browseOpen = false;
    onselectagent(id);
    await tick();
    if (id) inspectorHeading?.focus({ preventScroll: true });
    else focusBrowser();
  }
  function closeDetails() { return clearDetails(true); }
  async function clearDetails(restoreFocus: boolean) {
    const id = returnNode;
    picked = null;
    inspectingAgent = false;
    browseOpen = false;
    await tick();
    if (!restoreFocus) return;
    if (opener?.isConnected) opener.focus({ preventScroll: true });
    else if (!id || !canvas?.focusNode(id)) focusBrowser();
  }
  function reset() { kind = "all"; scope = "all"; enablement = "all"; picked = null; }
  function changeKind() { enablement = "all"; if (kind === "running") scope = "all"; picked = null; }
  async function closePanels() {
    coverageOpen = false;
    filtersOpen = false;
    await tick();
    if (panelOpener?.isConnected) panelOpener.focus({ preventScroll: true });
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

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (Dismiss local panels after child controls handle their own Escape keys.) -->
<section id="agent-capabilities" class="capabilities" aria-label="MCP and skills workspace" aria-busy={loading} bind:this={workspaceElement} {onkeydown}>
  <header class="workspace-head">
    <div class="workspace-title"><span class="title-icon"><Icon name="plug" size={19} /></span><div><h2>MCP &amp; skills {#if demo}<span class="preview">Example inventory</span>{/if}</h2><p>Explore the capabilities around your agents. Select a card to inspect it.</p></div></div>
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
    <div class="workspace">
      <aside class="rail" class:is-open={showDetails || browseOpen} aria-label={showDetails ? "Capability details" : "Capability agents"}>
        {#if showDetails}
          <header class="rail-head"><button class="back" onclick={closeDetails}><Icon name="arrow-left" size={14} />Agents</button><Button variant="ghost" size="xs" icon="x" aria-label="Close capability details" onclick={closeDetails} /></header>
          <div class="inspector">
            {#if chosen}
              <span class="eyebrow">{chosen.kind === "running" ? "Runtime observation" : chosen.kind === "mcp" ? "Configured MCP server" : "Available skill file"}</span>
              <h3 class="inspector-title" tabindex="-1" bind:this={inspectorHeading}><span class="detail-icon kind-{chosen.kind}"><Icon name={chosen.kind === "skill" ? "sparkles" : chosen.kind === "running" ? "cpu" : "plug"} size={20} /></span>{chosen.name}</h3>
              <p class="detail-intro">{chosen.kind === "running" ? "An observed process, independent of configuration." : chosen.kind === "skill" ? "SKILL.md is present in a supported skill directory." : "A named definition in supported configuration."}</p>
              {#if chosen.kind === "skill" && (chosen.definition as AgentSkill).description}<p class="skill-description">{(chosen.definition as AgentSkill).description}</p>{/if}
              <dl>
                <div><dt>Associated agents</dt><dd>{#each chosen.owners as owner}<button class="owner" onclick={() => selectAgent(owner.id)}>{owner.name} <code>PID {owner.pid}</code><Icon name="chevron" size={12} /></button>{/each}</dd></div>
                {#if chosen.kind === "running"}
                  <div><dt>Process ID</dt><dd><code>{chosen.tool.pid}</code></dd></div><div><dt>MCP classification</dt><dd>{EVIDENCE_LABEL[chosen.tool.evidence]}</dd></div>
                  <div><dt>Listening ports</dt><dd>{chosen.tool.ports.length ? chosen.tool.ports.map((port) => ":" + port).join(", ") : "None reported; this does not establish its transport."}</dd></div>
                  {#if chosen.tool.cwd}<div><dt>Working folder</dt><dd class="path">{chosen.tool.cwd}</dd></div>{/if}
                  <div><dt>Resources</dt><dd>{chosen.tool.cpu_percent.toFixed(1)}% CPU · {humanBytes(chosen.tool.memory_bytes)}</dd></div>
                {:else}
                  <div><dt>Scope</dt><dd>{scopeLabel[chosen.scope]}</dd></div><div><dt>Source file</dt><dd class="path">{chosen.sourcePath}</dd></div>
                  {#if chosen.projectPath}<div><dt>Project relationship</dt><dd class="path">{chosen.projectPath}</dd></div>{/if}
                  <div><dt>Evidence</dt><dd>{EVIDENCE_LABEL[chosen.definition.evidence]} file metadata</dd></div>
                  {#if chosen.kind === "mcp"}<div><dt>Transport declaration</dt><dd>{(chosen.definition as AgentMcpServer).transport === "unknown" ? "Not reported" : (chosen.definition as AgentMcpServer).transport.toUpperCase()}</dd></div><div><dt>Configuration enablement</dt><dd>{enabledLabel(chosen)}</dd></div>
                  {:else if chosen.name !== chosen.definition.name}<div><dt>Skill folder name</dt><dd>{chosen.definition.name}</dd></div>{/if}
                {/if}
              </dl>
              <div class="usage-note"><Icon name="shield" size={15} /><p>{chosen.kind === "running" ? "This process has not been matched to a configuration entry or invocation." : chosen.kind === "skill" ? "File presence does not prove this skill was loaded or activated. Only declared name/description from bounded frontmatter are retained; skill instructions are not collected or executed." : "Configured does not mean connected, loaded or invoked. Commands, arguments, URLs, headers and secret values are not shown."}</p></div>
              {#if onrecordedcalls}<Button size="sm" variant="ghost" icon="activity" onclick={onrecordedcalls}>Open recorded calls</Button>{/if}
            {:else if active}
              <span class="eyebrow">Agent</span><h3 class="inspector-title" tabindex="-1" bind:this={inspectorHeading}><span class="agent-avatar" aria-hidden="true" style={"--agent: " + agentColor(null, active.id, agentIds)}>{monogram(active.name)}</span>{active.name}</h3>
              <p class="detail-intro">PID {active.pid} · {active.vendor}</p>
              <div class="agent-counts"><span><Icon name="plug" size={15} /><b>{active.capabilities?.inspected ? active.capabilities.mcp_servers.length : "—"}</b>MCP definitions</span><span><Icon name="sparkles" size={15} /><b>{active.capabilities?.inspected ? active.capabilities.skills.length : "—"}</b>Skill files</span></div>
              <p class="agent-hint">Select a connected card on the map to see its purpose, source and scope.</p>
              <dl><div><dt>Inventory</dt><dd>{active.capabilities?.inspected ? "Supported sources inspected" : "Inventory not inspected"}</dd></div><div><dt>Resources</dt><dd>{active.cpu_percent.toFixed(1)}% CPU · {humanBytes(active.memory_bytes)}</dd></div></dl>
              <div class="usage-note"><Icon name="info" size={15} /><p>Declarations describe this agent's source layout. They do not establish loading, approval or usage by this running instance.</p></div>
              <Button size="sm" variant="ghost" icon="info" onclick={() => openPanel("coverage")}>View coverage</Button>
              <Button size="sm" variant="ghost" icon="layers" onclick={() => selectAgent(null)}>View all agents</Button>
            {/if}
          </div>
        {:else}
          <header class="rail-head"><h3>Agents <span>{agents.length}</span></h3>{#if browseOpen}<Button variant="ghost" size="xs" icon="x" aria-label="Close agent browser" onclick={closeDetails} />{:else}<span class="rail-hint">Select to explore</span>{/if}</header>
          <nav class="agent-list" aria-label="Capability agents">
            <button class="agent-choice all" class:on={!active} aria-pressed={!active} onclick={() => selectAgent(null)}><span class="agent-avatar" aria-hidden="true"><Icon name="layers" size={17} /></span><span><b>All agents</b><small>Shared definitions shown once</small></span><Icon name="chevron" size={13} /></button>
            {#each agents as agent (agent.id)}
              <button class="agent-choice" class:on={active?.id === agent.id} aria-pressed={active?.id === agent.id} aria-label={"Inspect agent card " + agent.name + ", PID " + agent.pid} onclick={() => selectAgent(agent.id)}>
                <span class="agent-avatar" aria-hidden="true" style={"--agent: " + agentColor(null, agent.id, agentIds)}>{monogram(agent.name)}</span><span><b>{agent.name}</b><small>PID {agent.pid}</small><small>{agent.capabilities?.inspected ? agent.capabilities.mcp_servers.length + " MCP · " + agent.capabilities.skills.length + " skills reported" : "Inventory not inspected"}</small></span><Icon name="chevron" size={13} />
              </button>
            {/each}
          </nav>
          <p class="rail-note">Follow a connection to discover a capability. Running candidates stay separate from file definitions.</p>
        {/if}
      </aside>
      <div class="map-area">
        <div class="map-tools">
          <div class="map-context"><button class="browse-button" aria-label="Browse agents" aria-expanded={browseOpen} onclick={browseAgents}><Icon name="sidebar" size={15} /></button><span><b>{active?.name ?? "All agents"}</b><small>{loading ? "Inspecting local sources…" : active ? "PID " + active.pid : agents.length + " agents · shared sources grouped"}</small></span>{#if active}<button class="all-link" onclick={() => selectAgent(null)}>Show all <Icon name="arrow" size={12} /></button>{/if}</div>
          <div class="map-filters"><ActionDock size="compact" hinge={false} animate={false} label="Capability type" bind:value={kind} onchange={changeKind} options={[{ value: "all", label: "All" }, { value: "mcp", label: "MCP servers", icon: "plug", count: inspected ? mcpCount : undefined }, { value: "skill", label: "Skills", icon: "sparkles", count: inspected ? skillCount : undefined }, { value: "running", label: "Running", icon: "cpu", count: processes.length }]} /><Button size="xs" variant={filtersOpen || scope !== "all" || enablement !== "all" ? "soft" : "ghost"} icon="filter" aria-expanded={filtersOpen} aria-controls="capability-filters" onclick={() => openPanel("filters")}>Filters</Button></div>
          {#if filtersOpen}<div class="filter-panel" id="capability-filters" bind:this={filterPanel}><Select label="Capability scope" prefix="Scope" labelHidden size="sm" disabled={kind === "running"} bind:value={scope} options={[{ value: "all", label: "All scopes" }, { value: "user", label: "User" }, { value: "project", label: "Project" }, { value: "local", label: "Local project" }]} /><Select label="MCP enablement" prefix="State" labelHidden size="sm" bind:value={enablement} disabled={kind === "skill" || kind === "running"} options={[{ value: "all", label: "All states" }, { value: "enabled", label: "Enabled MCP" }, { value: "disabled", label: "Disabled MCP" }, { value: "unknown", label: "Unknown MCP" }]} />{#if filtered}<Button size="xs" variant="ghost" onclick={reset}>Reset filters</Button>{/if}</div>{/if}
        </div>
        <div class="canvas-host">
          <CapabilityMap bind:this={canvas} agents={scoped} items={visible} {running} {agentIds} {selectedAgent} selectedId={chosen?.id ?? (showAgent ? active?.id ?? null : null)} filtered={narrowed} {dark} {reduced} onselect={select} onselectagent={selectAgent} />
          {#if !visible.length && !running.length}
            <div class="map-empty" role="status"><span class="empty-icon"><Icon name={loading ? "refresh" : narrowed ? "search" : "plug"} size={21} /></span><div><h3>{loading ? "Inspecting capability sources…" : kind === "running" ? query.trim() ? "No runtime matches" : "No runtime candidates" : narrowed ? "No inventory matches" : inspected ? "No entries reported" : "Inventory not inspected"}</h3><p>{kind === "running" ? "No MCP candidate processes were reported for this view. Remote or idle servers may have no local process." : narrowed ? "Try another name, source path or scope." : "Only supported, accessible sources are shown. Missing entries do not prove an agent has no capabilities."}</p><div class="empty-actions">{#if filtered}<button onclick={reset}>Reset filters</button>{/if}{#if query.trim() && onclearquery}<button onclick={onclearquery}>Clear search</button>{/if}{#if active}<button onclick={() => selectAgent(null)}>View all agents</button>{/if}<button onclick={() => openPanel("coverage")}>View coverage</button>{#if kind !== "running" && !inspected && !loading && onretry}<button onclick={onretry}>Inspect sources</button>{/if}</div></div></div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
  {#if coverageOpen}
    <section class="coverage-panel" id="capability-coverage" aria-label="Visibility limits">
      <header><div><span class="eyebrow">Local discovery</span><h3 tabindex="-1" bind:this={coverageHeading}>Coverage &amp; sources</h3></div><Button size="xs" variant="ghost" icon="x" aria-label="Close coverage" onclick={closePanels} /></header>
      <p>Configuration and skill files show what is present. Recorded calls provide separate evidence of use.</p>
      <div class="coverage-stat"><Icon name="code" size={15} /><span>{scoped.filter((agent) => agent.capabilities?.inspected).length} of {scoped.length} agent inventories inspected</span></div>
      {#if coverage.length}<ul>{#each coverage as item}<li><p>{item.text}</p>{#if item.owners.length}<small>{item.owners.length === 1 ? item.owners[0] : "Applies to " + item.owners.length + " agents"}</small>{/if}</li>{/each}</ul>{:else}<p>No additional visibility notes were reported.</p>{/if}
    </section>
  {/if}
</section>

<style>
  .capabilities { position: relative; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--bg); color: var(--text); container-type: inline-size; }
  h2, h3, p { margin: 0; } h2 { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: var(--fs-heading); line-height: var(--lh-heading); } h3 { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  button { font: inherit; cursor: pointer; } button:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; } .workspace-head { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3); padding: 14px 20px; border-bottom: 1px solid var(--border); background: var(--surface); flex: none; }
  .workspace-title { display: flex; align-items: center; gap: 11px; min-width: 0; } .title-icon { width: 38px; height: 38px; border-radius: 12px; display: grid; place-items: center; color: var(--accent); background: var(--accent-soft); flex: none; } .workspace-title p { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); margin-top: 3px; } .head-actions { display: flex; gap: 6px; flex: none; }
  .preview { color: var(--tone-blue); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-full); padding: 2px 7px; }
  .workspace { display: grid; grid-template-columns: 288px minmax(0, 1fr); flex: 1; min-height: 0; position: relative; } .rail { border-right: 1px solid var(--border); background: var(--surface); min-width: 0; overflow: auto; } .rail-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 53px; padding: 10px 16px; border-bottom: 1px solid var(--border); } .rail-head h3 { display: flex; gap: 8px; align-items: center; } .rail-head h3 span { color: var(--muted); font-weight: var(--fw-medium); font-size: var(--fs-caption); } .rail-hint { color: var(--muted); font-size: var(--fs-caption); }
  .agent-list { display: grid; gap: 8px; padding: 12px; } .agent-choice { display: flex; width: 100%; align-items: center; gap: 10px; border: 1px solid var(--border); border-radius: 12px; padding: 12px; background: var(--surface); color: var(--text-2); text-align: left; box-shadow: var(--shadow-sm); } .agent-choice:hover { background: var(--row-hover); border-color: var(--border-strong); } .agent-choice.on { background: var(--row-selected); border-color: var(--accent); } .agent-choice > span:nth-child(2) { display: grid; gap: 3px; flex: 1; min-width: 0; } .agent-choice b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; } .agent-choice small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .agent-choice > :global(svg) { color: var(--muted); flex: none; }
  .agent-avatar { --agent: var(--accent); display: grid; place-items: center; flex: none; width: 36px; height: 36px; color: var(--agent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); border-radius: 11px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); } .rail-note { padding: 5px 20px 20px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .back { display: flex; align-items: center; gap: 6px; border: 0; padding: 4px 0; color: var(--text-2); background: transparent; font-size: var(--fs-body-sm); } .inspector { padding: 22px 18px; } .eyebrow { display: block; color: var(--muted); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; margin-bottom: 8px; } .inspector-title { display: flex; gap: 10px; align-items: center; font-size: var(--fs-heading); line-height: var(--lh-heading); overflow-wrap: anywhere; outline: none; } .detail-icon { display: grid; place-items: center; width: 38px; height: 38px; flex: none; border-radius: 12px; color: var(--accent); background: var(--accent-soft); } .kind-skill { color: var(--tone-amber); background: color-mix(in srgb, var(--tone-amber) 12%, var(--surface)); } .kind-running { color: var(--tone-green); background: color-mix(in srgb, var(--tone-green) 12%, var(--surface)); } .detail-intro, .agent-hint { margin-top: 12px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .skill-description { margin-top: 16px; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  dl { display: grid; gap: 17px; margin: 24px 0; } dl > div { display: grid; gap: 5px; } dt { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } dd { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; } .path { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); user-select: text; } .owner { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; width: 100%; border: 0; padding: 5px 0; text-align: left; color: var(--text-2); background: transparent; font-size: var(--fs-body-sm); } .owner code { color: var(--muted); font-size: var(--fs-caption); } .owner > :global(svg) { margin-left: auto; } .usage-note { display: flex; gap: 8px; align-items: flex-start; border-top: 1px solid var(--border); padding-top: 16px; margin-bottom: 12px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .usage-note > :global(svg) { flex: none; margin-top: 2px; } .agent-counts { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 20px; } .agent-counts > span { display: grid; grid-template-columns: auto 1fr; gap: 6px; padding: 12px; background: var(--surface-2); border-radius: 10px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .agent-counts b { color: var(--text); font-size: var(--fs-heading); line-height: var(--lh-heading); }
  .map-area { display: flex; flex-direction: column; min-width: 0; min-height: 0; } .map-tools { position: relative; z-index: 3; display: flex; align-items: center; flex-wrap: wrap; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid var(--border); background: var(--surface); flex: none; } .map-context { display: flex; align-items: center; gap: 9px; min-width: 0; } .map-context > span { display: grid; gap: 2px; } .map-context b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .map-context small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .browse-button { display: none; border: 1px solid var(--border); border-radius: var(--r-sm); width: 30px; height: 30px; place-items: center; color: var(--text-2); background: var(--surface); } .all-link { display: flex; gap: 4px; align-items: center; margin-left: 8px; border: 0; background: transparent; color: var(--accent); font-size: var(--fs-caption); } .map-filters { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; min-width: 0; }
  .filter-panel { position: absolute; top: calc(100% + 7px); right: 16px; display: flex; gap: 9px; align-items: center; flex-wrap: wrap; max-width: calc(100% - 32px); padding: 14px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); box-shadow: var(--shadow-lg); } .filter-panel :global(.field) { min-width: 0; } .canvas-host { flex: 1; min-height: 0; position: relative; }
  .map-empty { position: absolute; z-index: 2; bottom: 60px; left: 50%; transform: translateX(-50%); display: flex; gap: 12px; width: min(430px, calc(100% - 40px)); padding: 16px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); box-shadow: var(--shadow-sm); } .empty-icon { color: var(--muted); flex: none; padding-top: 2px; } .map-empty p { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); margin-top: 5px; } .empty-actions { display: flex; flex-wrap: wrap; gap: 8px 14px; margin-top: 10px; } .empty-actions button { border: 0; padding: 0; background: transparent; color: var(--accent); font-size: var(--fs-caption); line-height: var(--lh-caption); text-decoration: underline; text-underline-offset: 3px; }
  .coverage-panel { position: absolute; z-index: 8; top: 64px; right: 16px; width: min(420px, calc(100% - 32px)); max-height: calc(100% - 80px); overflow: auto; padding: 20px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); box-shadow: var(--shadow-lg); } .coverage-panel header { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; } .coverage-panel > p { margin-top: 12px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .coverage-stat { display: flex; gap: 8px; align-items: center; margin-top: 16px; padding: 10px; border-radius: 9px; color: var(--text-2); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); } .coverage-panel ul { display: grid; gap: 12px; list-style: none; padding: 0; margin: 16px 0 0; } .coverage-panel li { padding-top: 12px; border-top: 1px solid var(--border); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); } .coverage-panel small { display: block; color: var(--muted); margin-top: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .notice { display: flex; align-items: center; gap: 8px; padding: 8px 16px; } .empty { flex: 1; display: flex; flex-direction: column; gap: 12px; align-items: center; justify-content: center; text-align: center; padding: 24px; color: var(--muted); } .empty h3 { color: var(--text-2); } .empty p { max-width: 480px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  @container (max-width: 1100px) { .workspace { grid-template-columns: 250px minmax(0, 1fr); } .workspace-head { padding-inline: 14px; } .workspace-title p { display: none; } .map-tools { align-items: flex-start; } }
  @container (max-width: 960px) { .workspace-head { padding: 10px 12px; flex-wrap: wrap; gap: 8px; } .workspace-title { gap: 8px; } .title-icon { width: 30px; height: 30px; border-radius: 9px; } .head-actions { margin-left: auto; } .workspace { grid-template-columns: minmax(0, 1fr); } .rail { display: none; } .rail.is-open { display: block; position: absolute; z-index: 6; top: 10px; left: 10px; bottom: 10px; width: min(320px, calc(100% - 20px)); border: 1px solid var(--border); border-radius: 14px; box-shadow: var(--shadow-lg); } .browse-button { display: grid; } .map-tools { padding: 10px 12px; gap: 9px; } .map-filters { width: 100%; justify-content: space-between; gap: 4px; } .map-filters :global(.action-dock) { min-width: 0; } .map-empty { bottom: 60px; width: calc(100% - 28px); padding: 12px; } .coverage-panel { top: 96px; max-height: calc(100% - 110px); } .preview { font-size: var(--fs-caption); } }
</style>
