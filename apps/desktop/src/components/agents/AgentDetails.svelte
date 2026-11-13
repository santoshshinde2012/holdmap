<script lang="ts">
  // The selected agent in full, in four sections beside the map: Overview (memory, access,
  // folders, ports, configuration, tools, processes), MCP & skills, Flow and Security, each
  // scoped to this agent with a way into the full workspace.
  import Icon from "../Icon.svelte";
  import Tabs, { panelId, tabId } from "../ui/Tabs.svelte";
  import AgentLocations from "./AgentLocations.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { TRUST_LABEL } from "../../lib/trust";
  import { SEVERITY_ORDER } from "../../lib/analyzers";
  import { activityRuns } from "../../lib/agent-flow/build";
  import { agentPids, agentSocketLinks, spanBelongsTo } from "../../lib/agent-scope";
  import type { ActivitySnapshot, AgentFlowReport } from "../../lib/agent-flow/model";
  import AgentTools from "./AgentTools.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import AgentConfigPanel from "./AgentConfigPanel.svelte";
  import { configSummary } from "../../lib/agent-config";
  import { agentCounts, agentFindings, countsText, total } from "../../lib/analyzers";
  import { redactHome } from "../../lib/report-export";
  import { scopeLabel } from "../../lib/run-view";
  import { EvidenceChip } from "../ui";
  import StatStrip from "../ui/StatStrip.svelte";
  import AgentMemoryPanel from "./AgentMemoryPanel.svelte";
  import { evidenceChip } from "../../lib/evidence";
  import { detailStats, factRows } from "../../lib/agent-details";
  import { humanBytes, tildify } from "../../lib/format";
  import { EVIDENCE_LABEL, accessHeadline, agentMeta, counts, folderSourceLabel, stoppableEntries, monogram, processRows } from "../../lib/agents";
  import AgentRelations from "./AgentRelations.svelte";
  import type { Agent, AgentMcpServer, AgentProcess, AgentsReport, PortEntry } from "../../lib/types";
  import type { TabItem } from "../ui/Tabs.svelte";
  type DetailTab = "overview" | "capabilities" | "flow" | "security";

  let {
    agent,
    report,
    entries,
    color,
    onclose,
    onentry,
    onreveal,
    oneditor,
    onfolder,
    onstop,
    onstopall,
    onnavigate,
    oncapabilities,
    oninsights,
    oninventory,
    onflow,
    flowReport = null,
    activity = null,
    tab = $bindable("overview"),
    now = Date.now(),
  }: {
    agent: Agent;
    report: AgentsReport;
    entries: PortEntry[];
    color: string;
    onclose?: () => void;
    onentry: (e: PortEntry) => void;
    onreveal: (path: string) => void | Promise<void | boolean>;
    oneditor: (path: string) => void | Promise<void | boolean>;
    onfolder?: (path: string) => void;
    onstop: (e: PortEntry) => void;
    onstopall: () => void;
    onnavigate?: (id: string) => void;
    oncapabilities?: () => void;
    oninsights?: () => void;
    oninventory?: () => void;
    /** Open the full Flow workspace (scoped to this agent). */
    onflow?: () => void;
    /** Observed process and socket relationships, when Flow data has been read. */
    flowReport?: AgentFlowReport | null;
    /** Reported activity from the opt-in recorder, when available. */
    activity?: ActivitySnapshot | null;
    /** The active section; kept by the parent so switching agents keeps it. */
    tab?: DetailTab;
    now?: number;
  } = $props();

  let heading = $state<HTMLElement | undefined>();
  let folderBusy = $state(false);
  let folderError = $state<string | null>(null);
  async function quickFolder(kind: "reveal" | "editor", path: string) {
    if (folderBusy) return;
    folderBusy = true;
    folderError = null;
    try { await (kind === "reveal" ? onreveal : oneditor)(path); }
    catch (e) { folderError = String(e); }
    finally { folderBusy = false; }
  }
  const jumpTabs = $derived([
    ["memory", "Memory"],
    ["access", "Access"],
    ...(agent.folders.length || agent.more_folders ? [["folders", "Folders"]] : []),
    ["procs", "Processes"],
  ] as const);
  const base = $derived(`agent-${agent.id.replace(/[^a-zA-Z0-9_-]/g, "-")}`);
  const caps = $derived(agent.capabilities);
  const capCount = $derived(caps?.inspected ? caps.mcp_servers.length + caps.more_mcp_servers + caps.skills.length + caps.more_skills : null);
  const pids = $derived(agentPids(agent));
  const sockets = $derived(agentSocketLinks(flowReport, agent));
  const runs = $derived(activityRuns(activity?.spans ?? []).filter((r) => r.spans.some((s) => spanBelongsTo(s, agent, pids))));
  const ownSpans = $derived(runs.flatMap((r) => r.spans.filter((s) => spanBelongsTo(s, agent, pids))));
  const ownCalls = $derived(ownSpans.filter((s) => s.kind === "tool").length);
  const flowCount = $derived(agent.links.length + agent.more_links + ownCalls);
  const openFindings = $derived(agentFindings(insightsStore.insights, agent.product));
  const allFindings = $derived(agentFindings(insightsStore.insights, agent.product, true));
  const reviewedFindings = $derived(allFindings.filter((x) => x.finding.state !== "open"));
  const severity = $derived(agentCounts(insightsStore.insights, agent.product));
  const trust = $derived(trustStore.forAgent(agent.product));
  const securityCount = $derived(total(severity) + trust.length);
  const sectionTabs = $derived<TabItem<DetailTab>[]>([
    { id: "overview", label: "Overview", icon: "panel" },
    { id: "capabilities", label: "MCP & skills", icon: "plug", count: capCount },
    { id: "flow", label: "Flow", icon: "activity", count: flowCount || null },
    { id: "security", label: "Security", icon: "shield", count: securityCount || null, alert: severity.critical + severity.high > 0 },
  ]);
  // Skill trust is read once for the sections that show it; the store dedupes the request.
  $effect(() => { if ((tab === "security" || tab === "capabilities") && !trustStore.report && !trustStore.busy) void trustStore.load(); });

  let capFilter = $state<"all" | "mcp" | "skills">("all");
  let capQuery = $state("");
  const capNeedle = $derived(capQuery.trim().toLocaleLowerCase());
  const capMatch = (...parts: (string | null | undefined)[]) => !capNeedle || parts.filter(Boolean).join(" ").toLocaleLowerCase().includes(capNeedle);
  const servers = $derived(capFilter === "skills" ? [] : (caps?.mcp_servers ?? []).filter((m) => capMatch(m.name, m.transport, m.launch?.package, m.remote_host, m.source_path)));
  const skills = $derived(capFilter === "mcp" ? [] : (caps?.skills ?? []).filter((k) => capMatch(k.name, k.declared_name, k.description, k.source_path)));
  let copiedPath = $state<string | null>(null);
  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copiedPath = path;
      setTimeout(() => { if (copiedPath === path) copiedPath = null; }, 1600);
    } catch { copiedPath = null; }
  }
  const launchText = (m: AgentMcpServer) => m.launch ? `${m.launch.runner}${m.launch.package ? ` ${m.launch.package}` : ""}` : m.remote_host ? `${m.transport.toUpperCase()} · ${m.remote_host}` : m.transport === "unknown" ? "Launch unknown" : m.transport.toUpperCase();
  const enabledText = (m: AgentMcpServer) => m.enabled === true ? "Enabled" : m.enabled === false ? "Disabled" : "Unknown";
  const enabledTone = (m: AgentMcpServer) => m.enabled === true ? "on" : m.enabled === false ? "off" : "unknown";
  const capScope = (scope: string) => scope === "user" ? "User" : scope === "project" ? "Project" : scope === "local" ? "Local" : scope;
  const spanAgo = (ms: number | null) => ms == null ? "time not reported" : new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const head = $derived(accessHeadline(agent));
  const c = $derived(counts(agent));
  const facts = $derived(factRows(agent));
  const stats = $derived(detailStats(agent));
  const cfg = $derived(configSummary(agent));
  const tree = $derived(processRows(agent));
  const entry = (id: string | null) => entries.find((e) => e.id === id) ?? null;
  const stoppable = $derived(stoppableEntries(agent, entries));
  const children = $derived(tree.filter(({ process }) => process.role === "child").map(({ process }) => process));
  const showSecurity = $derived(insightsStore.analyzers.some((a) => a.state === "approved") || !!insightsStore.insights?.results.length);
  const maxProcMem = $derived(Math.max(1, ...tree.map(({ process }) => process.memory_bytes)));

  function role(p: AgentProcess): string {
    return p.role === "helper" ? "helper" : p.role === "child" ? "tool / app it started" : "agent";
  }
  /** Focus the name so screen readers announce the newly selected agent. */
  export function focusHeading() { heading?.focus({ preventScroll: true }); }
</script>

{#snippet rowBody(icon: string, label: string, sub: string)}
  <span class="ic"><Icon name={icon} size={13} /></span>
  <span class="row-main"><span class="row-label">{label}</span><span class="row-sub mono">{sub}</span></span>
{/snippet}

{#snippet portsSection()}
  {#if agent.ports.length}
    <section class="sec-box" aria-labelledby="ports-head-{base}-{tab}">
      <div class="ports-head">
        <h4 id="ports-head-{base}-{tab}"><Icon name="server" size={13} />Ports it serves <span class="sec-n">{agent.ports.length}</span></h4>
      </div>
      <ul class="rows">
        {#each agent.ports as p}
          {@const e = entry(p.entry_id)}
          <li class="port-row">
            {#if e}
              <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane">{@render rowBody("server", p.project ?? p.framework ?? p.process ?? p.label, `:${p.port} · ${p.role === "dev_server" ? "dev server" : p.role === "agent" ? "agent's own port" : "service"}${p.exposure === "all_interfaces" ? " · reachable from the network" : p.exposure === "specific" ? " · bound to a specific interface" : ""}`)}</button>
            {:else}
              <span class="row-btn static">{@render rowBody("server", p.project ?? p.framework ?? p.process ?? p.label, `:${p.port} · ${p.role === "dev_server" ? "dev server" : p.role === "agent" ? "agent's own port" : "service"}`)}</span>
            {/if}
            {#if e && !e.protected && (p.role === "dev_server" || p.role === "service")}
              <button type="button" class="act stop" title="Stop :{e.port}" onclick={() => onstop(e)}><Icon name="stop" size={12} />Stop</button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

{#snippet openFull(label: string, icon: string, run: (() => void) | undefined)}
  {#if run}<button type="button" class="open-full" onclick={run}><Icon name={icon} size={13} />{label}<Icon name="arrow" size={12} /></button>{/if}
{/snippet}

<div class="details" style="--agent: {color}">
  <header class="dhead">
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <div class="who">
      <h3 class="name" tabindex="-1" bind:this={heading}>{agent.name}</h3>
      <span class="meta">{agentMeta(agent, now)}</span>
    </div>
    {#if onclose}<button type="button" class="x" aria-label="Close agent details" title="Close (Esc)" onclick={onclose}><Icon name="x" size={14} /></button>{/if}
    <span class="pill lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.text}</span>
  </header>

  <StatStrip compact label="{agent.name} summary" {stats} />

  {#if stoppable.length}
    <div class="actions" role="group" aria-label="Agent actions">
      <button type="button" class="act stop-all" title="Review a plan to stop every unprotected port this agent started" onclick={onstopall}><Icon name="stop" size={12} />Stop all ({stoppable.length})</button>
    </div>
  {/if}

  <div class="section-tabs">
    <Tabs tabs={sectionTabs} bind:value={tab} {base} label="Agent sections" />
  </div>

  <div class="tab-panel" role="tabpanel" id={panelId(base, tab)} aria-labelledby={tabId(base, tab)}>
  {#if tab === "overview"}
    <nav class="jump-nav" aria-label="Jump to section">
      {#each jumpTabs as [key, label] (key)}
        <button type="button" onclick={() => document.getElementById(`section-${key}-${agent.id}`)?.scrollIntoView?.({ behavior: "smooth", block: "start" })}>{label}</button>
      {/each}
    </nav>

    <AgentRelations {agent} {report} {onnavigate} />

    <AgentMemoryPanel {agent} {report} />

    <section class="sec-box" id="section-access-{agent.id}" aria-labelledby="access-head-{agent.id}">
      <h4 id="access-head-{agent.id}"><Icon name="lock" size={13} />Access</h4>
      <dl class="facts">
        {#each facts as f (f.topic)}
          <div class="fact lv-{f.level}" title={f.full}>
            <dt>{f.label}</dt>
            <dd><span class="lv">{f.levelText}</span><span class="sum">{f.text}</span>{#if f.inferred && f.level !== "unknown"}<EvidenceChip {...evidenceChip("inferred")} />{/if}</dd>
          </div>
        {/each}
      </dl>
    </section>

    {#if agent.folders.length || agent.more_folders}
      <section class="sec-box" id="section-folders-{agent.id}" aria-labelledby="folders-head-{agent.id}">
        <h4 id="folders-head-{agent.id}"><Icon name="folder" size={13} />Folders <span class="sec-n">{agent.folders.length}</span></h4>
        <ul class="rows">
          {#each agent.folders as f}
            <li class="folder">
              <button type="button" class="row-btn" aria-label="Folder actions for {f.path}" disabled={!onfolder || folderBusy} onclick={() => onfolder?.(f.path)}>
                <span class="ic"><Icon name="folder" size={13} /></span>
                <span class="row-main">
                  <span class="row-label">{f.label}{#if f.evidence !== "observed"}<em> · {EVIDENCE_LABEL[f.evidence]}</em>{/if}</span>
                  <span class="row-sub mono" title={f.path}>{tildify(f.path)}</span>
                  <span class="tags"><span class="tag">{folderSourceLabel(f.source)}</span>{#if f.project?.git_branch}<span class="tag mono branch">⎇ {f.project.git_branch}</span>{/if}{#if f.privacy_area}<span class="tag privacy">{f.privacy_area}</span>{/if}{#if f.pids.length}<span class="tag">{f.pids.length} proc</span>{/if}</span>
                  {#if f.note}<span class="row-note">{f.note}</span>{/if}
                </span>
              </button>
              <div class="acts" role="group" aria-label="Open {f.label}">
                <button type="button" class="act" title="Open in file manager" disabled={folderBusy} onclick={() => quickFolder("reveal", f.path)}><Icon name="folder" size={12} />Reveal</button>
                <button type="button" class="act" title="Open in editor" disabled={folderBusy} onclick={() => quickFolder("editor", f.path)}><Icon name="code" size={12} />Editor</button>
              </div>
            </li>
          {/each}
          {#if agent.more_folders}<li class="more">+{agent.more_folders} folders not listed</li>{/if}
        </ul>
        {#if folderError}<p class="folder-error" role="alert">{folderError}</p>{/if}
      </section>
    {/if}

    {@render portsSection()}

    <div class="sec-box" id="section-config-{agent.id}">
      {#if cfg}
        <details class="cfg" open>
          <summary><span class="cfg-sum"><Icon name="sliders" size={13} />Configuration</span> <span>{cfg.running}/{cfg.mcp} MCP running · {cfg.skills + cfg.instructions} skills · {cfg.rules} rules</span></summary>
          <AgentConfigPanel {agent} />
        </details>
      {:else}
        <AgentConfigPanel {agent} oninspect={oninventory ?? oncapabilities} />
      {/if}
    </div>

    <AgentTools tools={agent.tools ?? []} more={agent.more_tools ?? 0} />

    {#if showSecurity}
      <section class="sec-box" aria-labelledby="sec-head-{agent.id}">
        <h4 id="sec-head-{agent.id}"><Icon name="shield" size={13} />Security</h4>
        <p class="insight-line">{total(severity) ? `Open findings: ${countsText(severity, 5)}` : "No open findings"}{#if oninsights}{" "}<button type="button" class="link" onclick={oninsights}>Open Security</button>{/if}</p>
      </section>
    {/if}

    <section class="sec-box" id="section-procs-{agent.id}" aria-labelledby="procs-head-{agent.id}">
      <h4 id="procs-head-{agent.id}"><Icon name="cpu" size={13} />Processes <span class="sec-n">{c.processes}</span></h4>
      <code class="cmd" title={agent.command}>{agent.command}</code>
      {#if children.length}
        <p class="group-label">Tools &amp; apps it started ({children.length})</p>
        <ul class="procs">
          {#each children as p}
            {@const pct = Math.max(4, Math.round((p.memory_bytes / maxProcMem) * 100))}
            <li>
              <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
              <span class="prole" title={p.command}>{p.command || role(p)}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
              <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
              <span class="pbar" style="width: {pct}%" aria-hidden="true"></span>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="group-label">Agent &amp; helpers</p>
      <ul class="procs">
        {#each tree.filter(({ process }) => process.role !== "child") as { process: p, depth } (p.pid)}
          {@const pct = Math.max(4, Math.round((p.memory_bytes / maxProcMem) * 100))}
          <li style="padding-left: {depth * 10}px" title={p.command}>
            <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
            <span class="prole">{role(p)}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
            <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
            <span class="pbar" style="width: {pct}%" aria-hidden="true"></span>
          </li>
        {/each}
        {#if agent.more_processes}<li class="more">+{agent.more_processes} more processes</li>{/if}
      </ul>
    </section>

  {:else if tab === "capabilities"}
    <!-- Config & layers: where this agent's configuration lives, by precedence. -->
    <div class="sec-box"><AgentLocations {agent} oninspect={oninventory ?? oncapabilities} /></div>

    <section class="sec-box" aria-labelledby="caps-head-{base}">
      <h4 id="caps-head-{base}"><Icon name="plug" size={13} />{agent.name}'s MCP servers &amp; skills{#if capCount != null}<span class="sec-n">{capCount}</span>{/if}</h4>
      {#if !caps?.inspected}
        <p class="note">MCP servers and skills for {agent.name} aren't read yet. Reading lists names and file paths only; nothing is run.</p>
        {#if oninventory ?? oncapabilities}<button type="button" class="act read" onclick={oninventory ?? oncapabilities}><Icon name="plug" size={12} />Read MCP &amp; skills</button>{/if}
      {:else}
        <div class="cap-tools">
          <div class="seg" role="group" aria-label="Show">
            {#each [["all", "All"], ["mcp", "MCP"], ["skills", "Skills"]] as const as [value, label] (value)}
              <button type="button" class:on={capFilter === value} aria-pressed={capFilter === value} onclick={() => (capFilter = value)}>{label}</button>
            {/each}
          </div>
          <label class="cap-search"><Icon name="search" size={12} /><span class="sr-only">Filter {agent.name}'s MCP servers and skills</span><input type="search" placeholder="Filter" bind:value={capQuery} /></label>
        </div>
        {#if servers.length}
          <h5 class="cap-group">MCP servers <span>{servers.length}</span></h5>
          <ul class="cap-list" aria-label="{agent.name} MCP servers">
            {#each servers as m (m.id)}
              <li class="cap-card">
                <div class="cap-top"><b>{m.name}</b><span class="badge {enabledTone(m)}">{enabledText(m)}</span>{#if m.runtime?.state === "running"}<span class="badge run">Running</span>{/if}<span class="tag">{capScope(m.scope)}</span></div>
                <span class="row-sub mono" title={launchText(m)}>{launchText(m)}</span>
                <div class="cap-path"><span class="mono" title={m.source_path}>{tildify(m.source_path)}</span><button type="button" class="act" aria-label="Copy path for {m.name}" onclick={() => copyPath(m.source_path)}><Icon name={copiedPath === m.source_path ? "check" : "copy"} size={11} />{copiedPath === m.source_path ? "Copied" : "Copy path"}</button></div>
              </li>
            {/each}
          </ul>
        {/if}
        {#if skills.length}
          <h5 class="cap-group">Skills <span>{skills.length}</span></h5>
          <ul class="cap-list" aria-label="{agent.name} skills">
            {#each skills as k (k.id)}
              <li class="cap-card">
                <div class="cap-top"><b>{k.declared_name ?? k.name}</b><span class="tag">{capScope(k.scope)}</span></div>
                <span class="cap-desc">{k.description ?? "No declared purpose"}</span>
                <div class="cap-path"><span class="mono" title={k.source_path}>{tildify(k.source_path)}</span><button type="button" class="act" aria-label="Copy path for {k.declared_name ?? k.name}" onclick={() => copyPath(k.source_path)}><Icon name={copiedPath === k.source_path ? "check" : "copy"} size={11} />{copiedPath === k.source_path ? "Copied" : "Copy path"}</button></div>
              </li>
            {/each}
          </ul>
        {/if}
        {#if !servers.length && !skills.length}<p class="note">{capNeedle || capFilter !== "all" ? "Nothing matches this filter." : "No MCP servers or skills in the inspected locations."}</p>{/if}
        {#if caps.more_mcp_servers || caps.more_skills}<p class="more">+{caps.more_mcp_servers + caps.more_skills} more not listed</p>{/if}
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
    {@render openFull("Open full MCP & skills", "plug", oncapabilities)}

  {:else if tab === "flow"}
    <AgentRelations {agent} {report} {onnavigate} />
    {@render portsSection()}

    {#if agent.links.length || agent.more_links}
      <section class="sec-box" aria-labelledby="links-head-{base}">
        <h4 id="links-head-{base}"><Icon name="globe" size={13} />Talks to <span class="sec-n">{agent.links.length}</span></h4>
        <ul class="rows">
          {#each agent.links as l}
            {@const e = entry(l.entry_id)}
            <li>
              {#if e}
                <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane">{@render rowBody(l.kind === "remote" ? "globe" : "plug", l.label, `${l.kind === "remote" ? l.address : `:${l.port}`} · ×${l.connections}${l.service ? ` · ${l.service}` : ""}`)}</button>
              {:else}
                <span class="row-btn static">{@render rowBody(l.kind === "remote" ? "globe" : "plug", l.label, `${l.kind === "remote" ? l.address : `:${l.port}`} · ×${l.connections}${l.service ? ` · ${l.service}` : ""}`)}</span>
              {/if}
            </li>
          {/each}
          {#if agent.more_links}<li class="more">+{agent.more_links} links not listed</li>{/if}
        </ul>
      </section>
    {/if}

    <section class="sec-box" aria-labelledby="sockets-head-{base}">
      <h4 id="sockets-head-{base}"><Icon name="link" size={13} />Live socket links <span class="sec-n">{sockets.length}</span></h4>
      {#if !flowReport}
        <p class="note">Reading this agent's observed connections…</p>
      {:else if sockets.length}
        <ul class="rows socket-list">
          {#each sockets.slice(0, 12) as s (s.edge.id)}
            <li><span class="ic"><Icon name={s.to.kind === "remote" ? "globe" : "plug"} size={13} /></span><span class="row-main"><span class="row-label">{s.from.label} ↔ {s.to.label}</span><span class="row-sub mono">{s.to.address ?? "address unknown"}{s.to.port != null ? `:${s.to.port}` : ""} · {s.edge.connections} live {s.edge.connections === 1 ? "socket" : "sockets"}</span></span></li>
          {/each}
          {#if sockets.length > 12}<li class="more">+{sockets.length - 12} more in Flow</li>{/if}
        </ul>
      {:else}
        <p class="note">No live sockets observed for {agent.name} in the last scan.</p>
      {/if}
      <p class="fine">Sockets show that a connection exists, not messages or data direction.</p>
    </section>

    <section class="sec-box" aria-labelledby="calls-head-{base}">
      <h4 id="calls-head-{base}"><Icon name="activity" size={13} />Recorded calls <span class="sec-n">{ownCalls}</span></h4>
      {#if runs.length}
        <ul class="rows run-list">
          {#each runs.slice(0, 6) as r (r.id)}
            {@const mine = r.spans.filter((s) => spanBelongsTo(s, agent, pids))}
            <li><span class="ic"><Icon name="activity" size={13} /></span><span class="row-main"><span class="row-label">{r.spans.find((s) => s.kind === "agent")?.name ?? mine[0]?.name ?? r.label}</span><span class="row-sub">{r.label} · {mine.filter((s) => s.kind === "tool").length} tool calls · {mine.length} {mine.length === 1 ? "record" : "records"}{r.errors ? ` · ${r.errors} failed` : ""} · {spanAgo(r.first_at_ms)}</span></span></li>
          {/each}
        </ul>
      {:else}
        <p class="note">{activity ? `No recorded calls are attributed to ${agent.name}.` : "Recorded calls appear here when the opt-in recorder is on."}</p>
      {/if}
    </section>
    {@render openFull("Open full Flow", "activity", onflow)}

  {:else}
    <section class="sec-box" aria-labelledby="sev-head-{base}">
      <h4 id="sev-head-{base}"><Icon name="shield" size={13} />Findings for {agent.name}</h4>
      {#if !showSecurity}
        <p class="note">No security analyzers have run yet. Turn one on in Security to check {agent.name}'s skills and packages.</p>
      {:else}
        <div class="sev-row" aria-label="Open findings by severity">
          {#each SEVERITY_ORDER.filter((s) => severity[s] > 0) as s (s)}<span class="sev-chip"><span class="sev-pill {s}">{s}</span><b>{severity[s]}</b></span>{:else}<span class="sev-chip none">No open findings</span>{/each}
        </div>
        {#if openFindings.length}
          <ul class="findings" aria-label="Open findings">
            {#each openFindings as x (x.finding.fingerprint)}
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
        {#if reviewedFindings.length}
          <details class="reviewed"><summary>Reviewed or snoozed ({reviewedFindings.length})</summary>
            <ul class="findings">
              {#each reviewedFindings as x (x.finding.fingerprint)}<li class="closed"><span class="sev-pill {x.finding.severity}">{x.finding.severity}</span><span class="f-title">{x.finding.title}</span><small>{x.finding.state}</small></li>{/each}
            </ul>
          </details>
        {/if}
      {/if}
    </section>
    {#if trust.length}
      <section class="sec-box" aria-labelledby="sectrust-head-{base}">
        <h4 id="sectrust-head-{base}"><Icon name="alert" size={13} />Skill trust <span class="sec-n">{trust.length}</span></h4>
        <ul class="rows trust">
          {#each trust as t (t.subject.id)}<li><b>{t.subject.name}</b><span class="badge warn">{TRUST_LABEL[t.state]}</span></li>{/each}
        </ul>
      </section>
    {/if}
    {@render openFull("Open full Security", "shield", oninsights)}
  {/if}
  </div>
</div>


<style>
  .details { display: grid; gap: 10px; padding: 14px 16px 18px; min-width: 0; }
  .dhead { display: grid; grid-template-columns: auto 1fr auto; grid-template-areas: "tile who x" "pill pill pill"; gap: 8px 12px; align-items: center; }
  .tile { grid-area: tile; width: 40px; height: 40px; display: grid; place-items: center; border-radius: 12px; border: 1px solid color-mix(in srgb, var(--agent) 30%, transparent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); color: var(--agent); font-weight: var(--fw-semibold); font-size: var(--fs-body); }
  .who { grid-area: who; min-width: 0; display: grid; }
  .name { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; outline: none; }
  .meta { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .x { grid-area: x; align-self: start; display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 8px; background: transparent; color: var(--muted); cursor: pointer; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .pill { grid-area: pill; justify-self: start; display: inline-flex; align-items: center; gap: 6px; padding: 2px 10px; border-radius: 999px; border: 1px solid var(--border); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .pill i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
  .pill.lv-restricted i { background: var(--ok); }
  .pill.lv-standard i { background: var(--tone-blue); }
  .pill.lv-elevated { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 30%, var(--border)); background: color-mix(in srgb, var(--danger) 7%, var(--surface)); }
  .pill.lv-elevated i { background: var(--danger); }

  .actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .jump-nav { display: flex; flex-wrap: wrap; gap: 4px; padding: 4px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); }
  .jump-nav button { border: 0; border-radius: var(--r-sm); padding: 3px 8px; background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .jump-nav button:hover { background: var(--surface); color: var(--text); }
  .jump-nav button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }


  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; scroll-margin-top: 10px; }
  h4 { margin: 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  h4 :global(svg) { color: var(--agent); }
  .sec-n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }

  .cfg { display: grid; gap: 8px; }
  .cfg summary { cursor: pointer; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 8px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .cfg-sum { display: inline-flex; align-items: center; gap: 6px; }
  .cfg summary > span:last-child { font-weight: var(--fw-regular); color: var(--muted); }
  .cfg summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .group-label { margin: 4px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-weight: var(--fw-medium); }
  .ports-head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .ports-head h4 { flex: 1; }
  .facts { margin: 0; display: grid; }
  .fact { display: grid; grid-template-columns: 80px 1fr; gap: 10px; align-items: baseline; padding: 6px 0; border-top: 1px solid var(--border); }
  .fact:first-child { border-top: 0; padding-top: 0; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: baseline; min-width: 0; }
  .lv { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text); }
  .fact.lv-elevated .lv { color: var(--danger); }
  .fact.lv-restricted .lv { color: var(--ok); }
  .fact.lv-unknown .lv { color: var(--muted); }
  .sum { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .folder, .port-row { display: grid; gap: 2px; }
  .row-btn { width: 100%; display: flex; gap: 8px; align-items: flex-start; padding: 4px 6px; margin: 0 -6px; border: 0; border-radius: 8px; background: transparent; text-align: left; color: var(--text); font: inherit; }
  button.row-btn { cursor: pointer; }
  button.row-btn:hover { background: var(--surface-2); }
  button.row-btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  button.row-btn:disabled { cursor: default; }
  .row-btn.static { cursor: default; }
  .acts { display: flex; flex-wrap: wrap; gap: 4px; padding: 0 0 2px 28px; }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .folder-error { margin: 0; color: var(--danger); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
  .act.stop, .act.stop-all { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .act.stop:hover, .act.stop-all:hover { background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .port-row { grid-template-columns: 1fr auto; align-items: start; gap: 4px; }
  .port-row .act.stop { margin-top: 4px; }
  .ic { margin-top: 2px; color: var(--muted); display: inline-flex; }
  .row-main { min-width: 0; display: grid; }
  .row-label { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-label em { font-style: normal; color: var(--warn); font-weight: var(--fw-regular); }
  .row-sub, .row-note { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-note { white-space: normal; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 3px; }
  .tag { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); white-space: nowrap; }
  .tag.mono { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .tag.branch { color: var(--tone-violet); background: var(--tone-violet-bg); border-color: transparent; }
  .tag.privacy { color: var(--warn); background: var(--warn-soft); border-color: transparent; }
  .procs li { position: relative; display: grid; grid-template-columns: 1fr auto; grid-template-areas: "n f" "r r"; gap: 1px 8px; padding: 5px 0 6px; border-bottom: 1px solid var(--border); }
  .procs li:last-child { border-bottom: 0; }
  .pbar { position: absolute; left: 0; bottom: 0; height: 2px; border-radius: 1px; background: color-mix(in srgb, var(--agent) 55%, var(--surface)); opacity: 0.7; pointer-events: none; }
  .pname { grid-area: n; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); }
  .pfig { grid-area: f; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .prole { grid-area: r; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .pid { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); }
  .cmd { display: block; padding: 6px 8px; border-radius: 8px; background: var(--surface-2); color: var(--text-2); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .insight-line { margin: 0; color: var(--text-2); font-size: var(--fs-caption); }
  .insight-line .link { border: 0; background: none; padding: 0; color: var(--accent-text, var(--accent)); font: inherit; cursor: pointer; text-decoration: underline; }
  .section-tabs { position: sticky; top: 0; z-index: 3; margin: 0 -16px; background: var(--surface); }
  .section-tabs :global(.tabs) { padding: 0 8px; }
  .section-tabs :global(.tabs button) { padding: 0 7px; }
  .tab-panel { display: grid; gap: 10px; min-width: 0; outline: none; }
  .open-full { justify-self: start; display: inline-flex; align-items: center; gap: 6px; min-height: 30px; padding: 4px 12px; border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border)); border-radius: var(--r-full); background: var(--accent-softer, var(--surface-2)); color: var(--accent-text, var(--accent)); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); cursor: pointer; }
  .open-full:hover { background: var(--accent-soft); }
  .open-full:focus-visible, .act:focus-visible, .seg button:focus-visible, .cap-search:focus-within, .findings summary:focus-visible, .reviewed summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .fine { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .act.read { justify-self: start; }
  .cap-tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .seg { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); }
  .seg button { border: 0; border-radius: var(--r-full); padding: 2px 10px; background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); }
  .cap-search { flex: 1 1 120px; min-width: 0; display: inline-flex; align-items: center; gap: 6px; padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface); color: var(--muted); }
  .cap-search input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 2px 0; }
  .cap-group { margin: 4px 0 0; display: flex; gap: 6px; align-items: baseline; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .cap-group span { color: var(--muted); font-weight: var(--fw-regular); }
  .cap-list { gap: 6px; }
  .cap-card { display: grid; gap: 3px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); min-width: 0; }
  .cap-top { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; min-width: 0; }
  .cap-top b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; margin-right: auto; }
  .cap-desc { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .cap-path { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .cap-path .mono { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); }
  .cap-path .act { flex: none; min-height: 22px; }
  .badge { padding: 0 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); border: 1px solid transparent; white-space: nowrap; }
  .badge.on, .badge.run { color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, var(--surface)); }
  .badge.off { color: var(--text-2); background: var(--surface-3, var(--surface)); border-color: var(--border); }
  .badge.unknown { color: var(--muted); background: var(--surface); border-color: var(--border); }
  .badge.warn { color: var(--warn); background: var(--warn-soft); }
  .trust li { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .trust b { font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .socket-list li, .run-list li { display: flex; gap: 8px; align-items: flex-start; padding: 3px 0; }
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
