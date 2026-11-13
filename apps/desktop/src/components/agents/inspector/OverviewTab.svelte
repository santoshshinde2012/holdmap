<script lang="ts">
  // Overview: a compact summary of the selected agent (RAM, access, folders, configuration,
  // tools and processes). Relationships, ports and findings live in their own tabs.
  import Icon from "../../Icon.svelte";
  import AgentMemoryPanel from "../AgentMemoryPanel.svelte";
  import AgentConfigPanel from "../AgentConfigPanel.svelte";
  import AgentTools from "../AgentTools.svelte";
  import { EvidenceChip } from "../../ui";
  import { evidenceChip } from "../../../lib/evidence";
  import { factRows } from "../../../lib/agent-details";
  import { configSummary } from "../../../lib/agent-config";
  import { humanBytes, tildify } from "../../../lib/format";
  import { EVIDENCE_LABEL, counts, folderSourceLabel, processRows } from "../../../lib/agents";
  import type { Agent, AgentProcess, AgentsReport } from "../../../lib/types";

  let { agent, report, base, onreveal, oneditor, onfolder, oninspect }: {
    agent: Agent;
    report: AgentsReport;
    /** Sanitised id prefix shared with the tab bar. */
    base: string;
    onreveal: (path: string) => void | Promise<void | boolean>;
    oneditor: (path: string) => void | Promise<void | boolean>;
    onfolder?: (path: string) => void;
    oninspect?: () => void;
  } = $props();

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
    ["memory", "RAM"],
    ["access", "Access"],
    ...(agent.folders.length || agent.more_folders ? [["folders", "Folders"]] : []),
    ["procs", "Processes"],
  ] as const);
  const c = $derived(counts(agent));
  const facts = $derived(factRows(agent));
  const cfg = $derived(configSummary(agent));
  const tree = $derived(processRows(agent));
  const children = $derived(tree.filter(({ process }) => process.role === "child").map(({ process }) => process));
  const maxProcMem = $derived(Math.max(1, ...tree.map(({ process }) => process.memory_bytes)));
  function role(p: AgentProcess): string {
    return p.role === "helper" ? "helper" : p.role === "child" ? "tool / app it started" : "agent";
  }
</script>

{#snippet procRow(p: AgentProcess, sub: string, depth: number)}
  {@const pct = Math.max(4, Math.round((p.memory_bytes / maxProcMem) * 100))}
  <li style="padding-left: {depth * 10}px" title={p.command}>
    <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
    <span class="prole">{sub}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
    <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
    <span class="pbar" style="width: {pct}%" aria-hidden="true"></span>
  </li>
{/snippet}

<nav class="jump-nav" aria-label="Jump to section">
  {#each jumpTabs as [key, label] (key)}
    <button type="button" onclick={() => document.getElementById(`section-${key}-${base}`)?.scrollIntoView?.({ behavior: "smooth", block: "start" })}>{label}</button>
  {/each}
</nav>

<AgentMemoryPanel {agent} {report} />

<section class="sec-box" id="section-access-{base}" aria-labelledby="access-head-{base}">
  <h4 id="access-head-{base}"><Icon name="lock" size={13} />Access</h4>
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
  <section class="sec-box" id="section-folders-{base}" aria-labelledby="folders-head-{base}">
    <h4 id="folders-head-{base}"><Icon name="folder" size={13} />Folders <span class="sec-n">{agent.folders.length}</span></h4>
    <ul class="rows">
      {#each agent.folders as f (f.path)}
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

<div class="sec-box" id="section-config-{base}">
  {#if cfg}
    <details class="cfg" open>
      <summary><span class="cfg-sum"><Icon name="sliders" size={13} />Configuration</span> <span>{cfg.running}/{cfg.mcp} MCP running · {cfg.skills + cfg.instructions} skills · {cfg.rules} rules</span></summary>
      <AgentConfigPanel {agent} />
    </details>
  {:else}
    <AgentConfigPanel {agent} {oninspect} />
  {/if}
</div>

<AgentTools tools={agent.tools ?? []} more={agent.more_tools ?? 0} />

<section class="sec-box" id="section-procs-{base}" aria-labelledby="procs-head-{base}">
  <h4 id="procs-head-{base}"><Icon name="cpu" size={13} />Processes <span class="sec-n">{c.processes}</span></h4>
  <code class="cmd" title={agent.command}>{agent.command}</code>
  {#if children.length}
    <p class="group-label">Tools &amp; apps it started ({children.length})</p>
    <ul class="procs">
      {#each children as p (p.pid)}{@render procRow(p, p.command || role(p), 0)}{/each}
    </ul>
  {/if}
  <p class="group-label">Agent &amp; helpers</p>
  <ul class="procs">
    {#each tree.filter(({ process }) => process.role !== "child") as { process: p, depth } (p.pid)}{@render procRow(p, role(p), depth)}{/each}
    {#if agent.more_processes}<li class="more">+{agent.more_processes} more processes</li>{/if}
  </ul>
</section>

<style>
  .jump-nav { display: flex; flex-wrap: wrap; gap: 4px; padding: 4px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); }
  .jump-nav button { border: 0; border-radius: var(--r-sm); padding: 3px 8px; background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .jump-nav button:hover { background: var(--surface); color: var(--text); }
  .jump-nav button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; scroll-margin-top: 10px; }
  h4 { margin: 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .sec-n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .cfg { display: grid; gap: 8px; }
  .cfg summary { cursor: pointer; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 8px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .cfg-sum { display: inline-flex; align-items: center; gap: 6px; }
  .cfg summary > span:last-child { font-weight: var(--fw-regular); color: var(--muted); }
  .cfg summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
  .group-label { margin: 4px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-weight: var(--fw-medium); }
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
  .folder { display: grid; gap: 2px; }
  .row-btn { width: 100%; display: flex; gap: 8px; align-items: flex-start; padding: 4px 6px; margin: 0 -6px; border: 0; border-radius: 8px; background: transparent; text-align: left; color: var(--text); font: inherit; }
  button.row-btn { cursor: pointer; }
  button.row-btn:hover { background: var(--surface-2); }
  button.row-btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  button.row-btn:disabled { cursor: default; }
  .acts { display: flex; flex-wrap: wrap; gap: 4px; padding: 0 0 2px 28px; }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .folder-error { margin: 0; color: var(--danger); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
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
  .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
