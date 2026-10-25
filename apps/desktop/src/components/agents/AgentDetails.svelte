<script lang="ts">
  // The selected agent in full: who it is, its access in short rows, folders, ports it serves
  // (open details or plan a stop), links, configuration, tools, findings and processes. Shown in
  // the Agents side panel next to the map; nothing here stops anything without the stop plan.
  import Icon from "../Icon.svelte";
  import AgentTools from "./AgentTools.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import AgentConfigPanel from "./AgentConfigPanel.svelte";
  import { configSummary } from "../../lib/agent-config";
  import { agentCounts, agentFindings, countsText, total } from "../../lib/analyzers";
  import { redactHome } from "../../lib/report-export";
  import { scopeLabel } from "../../lib/security-view";
  import { EvidenceChip } from "../ui";
  import StatStrip from "../ui/StatStrip.svelte";
  import { evidenceChip } from "../../lib/evidence";
  import { detailStats, factRows } from "../../lib/agent-details";
  import { humanBytes, humanDuration, tildify } from "../../lib/format";
  import { EVIDENCE_LABEL, KIND_LABEL, accessHeadline, counts, folderSourceLabel, stoppableEntries, monogram, processRows, startedFrom } from "../../lib/agents";
  import type { Agent, AgentProcess, AgentsReport, PortEntry } from "../../lib/types";

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
  const head = $derived(accessHeadline(agent));
  const c = $derived(counts(agent));
  const facts = $derived(factRows(agent));
  const stats = $derived(detailStats(agent));
  const cfg = $derived(configSummary(agent));
  const parent = $derived(report.agents.find((a) => a.id === agent.parent) ?? null);
  const childAgents = $derived(startedFrom(report, agent.id));
  const tree = $derived(processRows(agent));
  const up = $derived(agent.started_at ? humanDuration(now / 1000 - agent.started_at) : null);
  const meta = $derived([KIND_LABEL[agent.kind], agent.vendor !== "unknown" ? agent.vendor : null, `pid ${agent.pid}`, up ? `up ${up}` : null].filter((p): p is string => !!p).join(" · "));
  const entry = (id: string | null) => entries.find((e) => e.id === id) ?? null;
  const stoppable = $derived(stoppableEntries(agent, entries));
  const children = $derived(tree.filter(({ process }) => process.role === "child").map(({ process }) => process));
  const showSecurity = $derived(insightsStore.analyzers.some((a) => a.state === "approved") || !!insightsStore.insights?.results.length);

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

<div class="details" style="--agent: {color}">
  <header class="dhead">
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <div class="who">
      <h3 class="name" tabindex="-1" bind:this={heading}>{agent.name}</h3>
      <span class="meta">{meta}</span>
    </div>
    {#if onclose}<button type="button" class="x" aria-label="Close agent details" title="Close (Esc)" onclick={onclose}><Icon name="x" size={14} /></button>{/if}
    <span class="pill lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.text}</span>
  </header>

  <StatStrip compact label="{agent.name} summary" {stats} />

  <div class="actions" role="group" aria-label="Agent actions">
    {#if oncapabilities}<button type="button" class="act" onclick={oncapabilities}><Icon name="plug" size={12} />MCP &amp; skills{#if agent.capabilities?.inspected} · {agent.capabilities.mcp_servers.length}{/if}</button>{/if}
    {#if oninsights && showSecurity}<button type="button" class="act" onclick={oninsights}><Icon name="shield" size={12} />Security</button>{/if}
    {#if stoppable.length}<button type="button" class="act stop-all" title="Review a plan to stop every unprotected port this agent started" onclick={onstopall}><Icon name="stop" size={12} />Stop all ({stoppable.length})</button>{/if}
  </div>

  {#if parent || childAgents.length}
    <p class="relations">
      {#if parent}<span>Started from {#if onnavigate}<button type="button" onclick={() => onnavigate?.(parent.id)}>{parent.name} <span class="mono">{parent.pid}</span></button>{:else}{parent.name} (pid {parent.pid}){/if}</span>{/if}
      {#if childAgents.length}<span>Started {#each childAgents as child, i}{#if i}, {/if}{#if onnavigate}<button type="button" onclick={() => onnavigate?.(child.id)}>{child.name} <span class="mono">{child.pid}</span></button>{:else}{child.name} (pid {child.pid}){/if}{/each}</span>{/if}
    </p>
  {/if}

  <h4>Access</h4>
  <dl class="facts">
    {#each facts as f (f.topic)}
      <div class="fact lv-{f.level}" title={f.full}>
        <dt>{f.label}</dt>
        <dd><span class="lv">{f.levelText}</span><span class="sum">{f.text}</span>{#if f.inferred && f.level !== "unknown"}<EvidenceChip {...evidenceChip("inferred")} />{/if}</dd>
      </div>
    {/each}
  </dl>

      {#if agent.folders.length || agent.more_folders}
        <h4>Folders</h4>
        <ul class="rows">
          {#each agent.folders as f}
            <li class="folder">
              <button type="button" class="row-btn" aria-label="Folder actions for {f.path}" disabled={!onfolder || folderBusy} onclick={() => onfolder?.(f.path)}>
                <span class="ic"><Icon name="folder" size={13} /></span>
                <span class="row-main">
                  <span class="row-label">{f.label}{#if f.evidence !== "observed"}<em> · {EVIDENCE_LABEL[f.evidence]}</em>{/if}</span>
                  <span class="row-sub">{tildify(f.path)} · {folderSourceLabel(f.source)}{f.project?.git_branch ? ` · ⎇ ${f.project.git_branch}` : ""}{f.privacy_area ? ` · ${f.privacy_area}` : ""}{f.pids.length ? ` · ${f.pids.length} proc` : ""}</span>
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
      {/if}

      {#if agent.ports.length}
        <div class="ports-head">
          <h4>Ports it serves</h4>

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
      {/if}

      {#if agent.links.length || agent.more_links}
        <h4>Talks to</h4>
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
      {/if}

      {#if cfg}
        <details class="cfg" open>
          <summary>Configuration <span>{cfg.running}/{cfg.mcp} MCP running · {cfg.skills + cfg.instructions} skills · {cfg.rules} rules</span></summary>
          <AgentConfigPanel {agent} />
        </details>
      {:else}
        <AgentConfigPanel {agent} oninspect={oninventory ?? oncapabilities} />
      {/if}

      <AgentTools tools={agent.tools ?? []} more={agent.more_tools ?? 0} />

      {#if insightsStore.analyzers.some((a) => a.state === "approved") || insightsStore.insights?.results.length}
        {@const found = agentCounts(insightsStore.insights, agent.product)}
        <h4>Security</h4>
        <p class="insight-line">{total(found) ? `Open findings: ${countsText(found, 5)}` : "No open findings"}{#if oninsights}{" "}<button type="button" class="link" onclick={oninsights}>Open Security</button>{/if}</p>
        {@const bySkill = [...new Map(agentFindings(insightsStore.insights, agent.product).map((x) => [x.finding.subject, x])).values()]}
        {#if bySkill.length}
          <ul class="skill-sec" aria-label="Skills with open findings" data-agent-skill-findings>
            {#each bySkill.slice(0, 6) as x (x.finding.subject)}
              <li><span class="sev-pill {x.finding.severity}">{x.finding.severity}</span><b>{x.subject?.name ?? x.finding.subject}</b><small>{scopeLabel(x.subject?.scope)} · {x.subject?.dir ? redactHome(x.subject.dir) : "folder unknown"}</small></li>
            {/each}
          </ul>
        {/if}
      {/if}

      <h4>Processes · {c.processes}</h4>
      <code class="cmd" title={agent.command}>{agent.command}</code>
      {#if children.length}
        <p class="group-label">Tools &amp; apps it started ({children.length})</p>
        <ul class="procs">
          {#each children as p}
            <li>
              <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
              <span class="prole" title={p.command}>{p.command || role(p)}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
              <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="group-label">Agent &amp; helpers</p>
      <ul class="procs">
        {#each tree.filter(({ process }) => process.role !== "child") as { process: p, depth } (p.pid)}
          <li style="padding-left: {depth * 10}px" title={p.command}>
            <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
            <span class="prole">{role(p)}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
            <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
          </li>
        {/each}
        {#if agent.more_processes}<li class="more">+{agent.more_processes} more processes</li>{/if}
      </ul>
    </div>

<style>
  .details { display: grid; gap: 10px; padding: 14px 16px 18px; min-width: 0; }
  .dhead { display: grid; grid-template-columns: auto 1fr auto; grid-template-areas: "tile who x" "pill pill pill"; gap: 8px 12px; align-items: center; }
  .tile { grid-area: tile; width: 40px; height: 40px; display: grid; place-items: center; border-radius: 12px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-weight: var(--fw-semibold); font-size: var(--fs-body); }
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
  .relations { margin: 0; display: grid; gap: 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .relations button { padding: 0; border: 0; background: transparent; color: var(--text-2); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  h4 { margin: 6px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--muted); text-transform: none; }
  .fact:first-child { border-top: 0; }

  .cfg summary { cursor: pointer; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .cfg summary span { font-weight: var(--fw-regular); color: var(--muted); }
  .cfg summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .group-label { margin: 6px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .ports-head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin-top: 6px; }
  .ports-head h4 { margin: 0; }
  .facts { margin: 0; display: grid; }
  .fact { display: grid; grid-template-columns: 84px 1fr; gap: 10px; align-items: baseline; padding: 6px 0; border-top: 1px solid var(--border); }
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
  .procs li { display: grid; grid-template-columns: 1fr auto; grid-template-areas: "n f" "r r"; gap: 0 8px; padding: 3px 0; }
  .pname { grid-area: n; } .pfig { grid-area: f; } .prole { grid-area: r; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pid { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); }
  .cmd { display: block; padding: 6px 8px; border-radius: 8px; background: var(--surface-2); color: var(--text-2); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pname { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); }
  .prole, .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .pfig { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .insight-line { margin: 0; color: var(--text-2); font-size: var(--fs-caption); }
  .insight-line .link { border: 0; background: none; padding: 0; color: var(--accent-text, var(--accent)); font: inherit; cursor: pointer; text-decoration: underline; }
  .skill-sec { list-style: none; margin: 0 0 8px; padding: 0; display: grid; gap: 4px; }
  .skill-sec li { display: flex; align-items: baseline; gap: 6px; min-width: 0; font-size: var(--fs-body-sm); }
  .skill-sec b { font-weight: var(--fw-medium); white-space: nowrap; }
  .skill-sec small { color: var(--muted); font-size: var(--fs-caption); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
