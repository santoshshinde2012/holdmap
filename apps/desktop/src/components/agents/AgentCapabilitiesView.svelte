<script lang="ts">
  import Icon from "../Icon.svelte";
  import { ActionDock, Button, Callout, Select } from "../ui";
  import { EVIDENCE_LABEL, monogram } from "../../lib/agents";
  import { humanBytes } from "../../lib/format";
  import type { AgentMcpServer, AgentSkill, AgentsReport, CapabilityScope } from "../../lib/types";
  import { capabilityItems, capabilityLabel, capabilityLimits, capabilityMatches, enabledLabel, filterCapabilities, pathName, runningMcpItems, scopeLabel, type CapabilitySelection, type InventoryKind, type InventoryState } from "../../lib/agent-capabilities";
  import { tick } from "svelte";

  let { report, query = "", selectedAgent = null, loading = false, error = null, demo = false, onselectagent, onclearquery, onretry, onrecordedcalls }: {
    report: AgentsReport | null; query?: string; selectedAgent: string | null;
    loading?: boolean; error?: string | null; demo?: boolean;
    onselectagent: (id: string | null) => void; onclearquery?: () => void;
    onretry?: () => void; onrecordedcalls?: () => void;
  } = $props();
  let kind = $state<InventoryKind>("all");
  let scope = $state<"all" | CapabilityScope>("all");
  let enablement = $state<InventoryState>("all");
  let picked = $state<string | null>(null);
  let detail: HTMLElement | undefined = $state();
  const agents = $derived(report?.agents ?? []);
  const active = $derived(agents.find((agent) => agent.id === selectedAgent) ?? null);
  const scoped = $derived(active ? [active] : agents);
  const inventory = $derived(capabilityItems(scoped));
  const visible = $derived(filterCapabilities(inventory, query, kind, scope, enablement));
  const running = $derived(runningMcpItems(scoped).filter((item) => capabilityMatches(item, query)));
  const limits = $derived([...new Set([...capabilityLimits(scoped), ...(report?.limits ?? [])])]);
  const inspected = $derived(scoped.some((agent) => agent.capabilities?.inspected));
  const chosen = $derived<CapabilitySelection | null>(visible.find((item) => item.id === picked) ?? running.find((item) => item.id === picked) ?? visible[0] ?? running[0] ?? null);
  const mcpCount = $derived(inventory.filter((item) => item.kind === "mcp").length);
  const skillCount = $derived(inventory.filter((item) => item.kind === "skill").length);
  const filtered = $derived(kind !== "all" || scope !== "all" || enablement !== "all");
  function reset() { kind = "all"; scope = "all"; enablement = "all"; picked = null; }
  async function select(id: string) {
    picked = id;
    await tick();
    if (detail && window.matchMedia("(max-width: 720px)").matches) detail.scrollIntoView({ block: "nearest" });
  }
  const ownerText = (item: CapabilitySelection) => item.owners.map((owner) => owner.name).filter((name, index, names) => names.indexOf(name) === index).join(", ");
</script>

<section id="agent-capabilities" class="capabilities" aria-label="MCP and skills workspace" aria-busy={loading}>
  <header class="workspace-head">
    <div><span class="eyebrow">Agent capabilities</span><h2>MCP &amp; skills {#if demo}<span class="preview">Example inventory</span>{/if}</h2><p>What is configured, what is present, and what is actually observed.</p></div>
    {#if onrecordedcalls}<Button size="sm" icon="activity" onclick={onrecordedcalls}>Recorded calls</Button>{/if}
  </header>
  <div class="evidence-strip" aria-label="Three separate evidence sources">
    <span><Icon name="code" size={13} /><b>Inventory</b> configuration names &amp; skill files</span>
    <span><Icon name="cpu" size={13} /><b>Runtime</b> observed processes</span>
    <span><Icon name="activity" size={13} /><b>Usage</b> requires reported calls</span>
  </div>
  {#if error}<div class="notice" role="alert"><Callout tone="warn" title="Couldn't refresh capability inventory">{error}</Callout>{#if onretry}<Button size="sm" icon="refresh" onclick={onretry}>Try again</Button>{/if}</div>{/if}
  {#if !report}
    <div class="empty" role="status"><Icon name="plug" size={28} /><h3>{error ? "Inventory unavailable" : "Loading agents…"}</h3><p>Capability sources are inspected only when this workspace is opened.</p></div>
  {:else if !agents.length}
    <div class="empty"><Icon name="bot" size={28} /><h3>No running agents to inspect</h3><p>Start a supported agent, then refresh. This inventory describes sources associated with recognized running agents; it is not a list of everything installed on this computer.</p>{#if onretry}<Button size="sm" icon="refresh" onclick={onretry}>Refresh agents</Button>{/if}{#if report.limits.length}<ul>{#each report.limits as limit}<li>{limit}</li>{/each}</ul>{/if}</div>
  {:else}
    <div class="workspace">
      <nav class="agent-rail" aria-label="Capability agents">
        <div class="rail-title">Inspect an agent <span>{agents.length}</span></div>
        <button class="agent-choice" class:on={!active} aria-pressed={!active} onclick={() => { picked = null; onselectagent(null); }}><span class="avatar"><Icon name="layers" size={16} /></span><span><b>All agents</b><small>Shared definitions shown once</small></span></button>
        {#each agents as agent (agent.id)}
          <button class="agent-choice" class:on={active?.id === agent.id} aria-pressed={active?.id === agent.id} aria-label="Inspect capabilities for {agent.name}, PID {agent.pid}" onclick={() => { picked = null; onselectagent(agent.id); }}>
            <span class="avatar">{monogram(agent.name)}</span><span><b>{agent.name}</b><small>PID {agent.pid}</small><small>{agent.capabilities?.inspected ? `${agent.capabilities.mcp_servers.length} MCP · ${agent.capabilities.skills.length} skills reported` : "Inventory not inspected"}</small></span>
          </button>
        {/each}
        <p class="rail-note">Declarations belong to an agent's source layout. They do not prove that this running instance loaded or used them.</p>
      </nav>
      <section class="inventory-pane" aria-label="Capability inventory">
        <header class="inventory-head"><div><h3>{active?.name ?? "All agents"}</h3><p>{loading ? "Inspecting supported sources…" : "Read-only metadata · no servers contacted or skills executed"}</p></div><span class="total">{inspected ? inventory.length : "—"} entries</span></header>
        <div class="filters">
          <ActionDock size="compact" hinge={false} animate={false} label="Capability type" bind:value={kind} onchange={() => { enablement = "all"; picked = null; }} options={[{ value: "all", label: "All" }, { value: "mcp", label: "MCP servers", icon: "plug", count: inspected ? mcpCount : undefined }, { value: "skill", label: "Skills", icon: "sparkles", count: inspected ? skillCount : undefined }]} />
          <div class="filter-selects"><Select label="Capability scope" labelHidden prefix="Scope" size="sm" bind:value={scope} options={[{ value: "all", label: "All scopes" }, { value: "user", label: "User" }, { value: "project", label: "Project" }, { value: "local", label: "Local project" }]} /><Select label="MCP enablement" labelHidden prefix="State" size="sm" bind:value={enablement} disabled={kind === "skill"} options={[{ value: "all", label: "All states" }, { value: "enabled", label: "Enabled MCP" }, { value: "disabled", label: "Disabled MCP" }, { value: "unknown", label: "Unknown MCP" }]} /></div>
          {#if query.trim()}<div class="search-note">Search: <b>{query}</b>{#if onclearquery}<button onclick={onclearquery}>Clear search</button>{/if}</div>{/if}
        </div>
        {#if chosen}
          <section class="relationship" aria-label={chosen.kind === "running" ? "Observed process relationship" : "Declared capability relationship"}>
            <div class="diagram-caption"><Icon name={chosen.kind === "running" ? "cpu" : "branch"} size={12} />{chosen.kind === "running" ? "Observed process relationship" : "Declared capability relationship"}</div>
            <ol class="diagram">
              <li><Icon name={chosen.kind === "running" ? "bot" : "code"} size={16} /><small>{chosen.kind === "running" ? "Agent" : "Source file"}</small><b title={chosen.kind === "running" ? ownerText(chosen) : chosen.sourcePath}>{chosen.kind === "running" ? ownerText(chosen) : pathName(chosen.sourcePath)}</b><span>{chosen.kind === "running" ? "owns a process" : "declares"}</span></li>
              <li class="diagram-focus"><Icon name={chosen.kind === "skill" ? "sparkles" : chosen.kind === "running" ? "cpu" : "plug"} size={16} /><small>{chosen.kind === "skill" ? "Skill file" : chosen.kind === "running" ? "Running process" : "MCP definition"}</small><b>{chosen.name}</b><span>{chosen.kind === "running" ? `PID ${chosen.tool.pid}` : ownerText(chosen)}</span></li>
              <li><Icon name={chosen.kind === "running" ? "server" : chosen.projectPath ? "folder" : "user"} size={16} /><small>{chosen.kind === "running" ? "Listeners" : "Declared scope"}</small><b>{chosen.kind === "running" ? chosen.tool.ports.length ? chosen.tool.ports.map((port) => `:${port}`).join(", ") : "No listening port" : chosen.projectPath ? pathName(chosen.projectPath) : scopeLabel[chosen.scope]}</b><span>{chosen.kind === "running" ? "not a call trace" : "not proof of use"}</span></li>
            </ol>
          </section>
        {/if}
        <div class="inventory-list">
          {#if visible.length}
            <ul class="records" aria-label="Configured MCP and skill files">
              {#each visible as item (item.id)}
                <li><button class="record" class:chosen={chosen?.id === item.id} aria-pressed={chosen?.id === item.id} aria-label={capabilityLabel(item)} onclick={() => select(item.id)}>
                  <span class="record-icon"><Icon name={item.kind === "mcp" ? "plug" : "sparkles"} size={16} /></span><span class="record-main"><b>{item.name}</b>{#if item.kind === "skill" && (item.definition as AgentSkill).description}<span class="skill-purpose">{(item.definition as AgentSkill).description}</span>{/if}<span>{ownerText(item)} <span aria-hidden="true">→</span> {item.projectPath ? pathName(item.projectPath) : `${scopeLabel[item.scope]} scope`}</span><small>{item.kind === "mcp" ? "Configured MCP" : "Skill file"} · {enabledLabel(item)}</small></span><span class="scope">{scopeLabel[item.scope]}</span><Icon name="chevron" size={13} />
                </button></li>
              {/each}
            </ul>
          {:else}
            <div class="empty compact" role="status"><Icon name={loading ? "refresh" : "search"} size={22} /><h3>{loading ? "Inspecting capability sources…" : query.trim() || filtered ? "No inventory matches" : inspected ? "No entries reported" : "Inventory not inspected"}</h3><p>{query.trim() || filtered ? "Try another name, source path or scope. Running processes remain separate below." : "Only supported, accessible sources are listed. Missing entries do not prove an agent has no capabilities; review the visibility limits below."}</p><div class="empty-actions">{#if filtered}<Button size="sm" onclick={reset}>Reset filters</Button>{/if}{#if query.trim() && onclearquery}<Button size="sm" onclick={onclearquery}>Clear search</Button>{/if}{#if !inspected && !loading && onretry}<Button size="sm" icon="refresh" onclick={onretry}>Inspect sources</Button>{/if}</div></div>
          {/if}
        </div>
        <section class="runtime" aria-label="Running MCP process observations">
          <header><h3><Icon name="cpu" size={13} />Running MCP candidates <span>{running.length}</span></h3><p>Observed processes, independently of configuration. MCP classification can be inferred.</p></header>
          {#if running.length}<ul>{#each running as item (item.id)}<li><button class="runtime-row" class:chosen={chosen?.id === item.id} aria-pressed={chosen?.id === item.id} aria-label="Inspect running process {item.name}, PID {item.tool.pid}" onclick={() => select(item.id)}><Icon name="cpu" size={13} /><span><b>{item.name}</b><small>{ownerText(item)} · {EVIDENCE_LABEL[item.tool.evidence]}</small></span><code>{item.tool.pid}</code></button></li>{/each}</ul>{:else}<p class="runtime-empty">{query.trim() ? "No running MCP candidate matches this search." : "No MCP candidate process was reported. Remote or idle configured servers may have no local process."}</p>{/if}
        </section>
        {#if limits.length}<details class="limits" open={!inventory.length}><summary><Icon name="info" size={13} />Visibility limits <span>{limits.length}</span></summary><ul>{#each limits as limit}<li>{limit}</li>{/each}</ul></details>{/if}
      </section>
      <aside class="detail-pane" aria-label="Capability details" bind:this={detail}>
        {#if chosen}
          <div class="detail-head"><span class="eyebrow">{chosen.kind === "running" ? "Runtime observation" : chosen.kind === "mcp" ? "Configured MCP server" : "Available skill file"}</span><h3><Icon name={chosen.kind === "skill" ? "sparkles" : chosen.kind === "running" ? "cpu" : "plug"} size={20} />{chosen.name}</h3><p>{chosen.kind === "running" ? "A process is running. Its purpose may be inferred." : chosen.kind === "skill" ? "SKILL.md is present in a supported skill directory." : "A named definition was found in supported configuration."}</p></div>
          {#if chosen.kind === "skill" && (chosen.definition as AgentSkill).description}<p class="skill-description">{(chosen.definition as AgentSkill).description}</p>{/if}
          <dl>
            <div><dt>Associated agents</dt><dd>{#each chosen.owners as owner}<span class="owner">{owner.name} <code>PID {owner.pid}</code></span>{/each}</dd></div>
            {#if chosen.kind === "running"}
              <div><dt>Process ID</dt><dd><code>{chosen.tool.pid}</code></dd></div><div><dt>MCP classification</dt><dd>{EVIDENCE_LABEL[chosen.tool.evidence]}</dd></div><div><dt>Listening ports</dt><dd>{chosen.tool.ports.length ? chosen.tool.ports.map((port) => `:${port}`).join(", ") : "None reported; this does not establish its transport."}</dd></div>{#if chosen.tool.cwd}<div><dt>Working folder</dt><dd class="path">{chosen.tool.cwd}</dd></div>{/if}<div><dt>Resources</dt><dd>{chosen.tool.cpu_percent.toFixed(1)}% CPU · {humanBytes(chosen.tool.memory_bytes)}</dd></div>
            {:else}
              <div><dt>Scope</dt><dd>{scopeLabel[chosen.scope]}</dd></div><div><dt>Source file</dt><dd class="path">{chosen.sourcePath}</dd></div>{#if chosen.projectPath}<div><dt>Project relationship</dt><dd class="path">{chosen.projectPath}</dd></div>{/if}<div><dt>Evidence</dt><dd>{EVIDENCE_LABEL[chosen.definition.evidence]} file metadata</dd></div>
              {#if chosen.kind === "mcp"}<div><dt>Transport declaration</dt><dd>{(chosen.definition as AgentMcpServer).transport === "unknown" ? "Not reported" : (chosen.definition as AgentMcpServer).transport.toUpperCase()}</dd></div><div><dt>Configuration enablement</dt><dd>{enabledLabel(chosen)}</dd></div>{:else if chosen.name !== chosen.definition.name}<div><dt>Skill folder name</dt><dd>{chosen.definition.name}</dd></div>{/if}
            {/if}
          </dl>
          <div class="usage-note"><Icon name="shield" size={16} /><div><b>Usage is separate evidence</b><p>{chosen.kind === "running" ? "This process has not been matched to a configuration entry or invocation." : chosen.kind === "skill" ? "File presence does not prove this skill was loaded or activated. Only declared name/description from bounded frontmatter are retained; skill instructions are not collected or executed." : "Configured does not mean connected, loaded or invoked. Commands, arguments, URLs, headers and secret values are not shown."}</p>{#if onrecordedcalls}<button onclick={onrecordedcalls}>Open recorded calls <Icon name="arrow" size={12} /></button>{/if}</div></div>
        {:else}
          <div class="empty compact"><Icon name="panel" size={26} /><h3>Inspect a capability</h3><p>Select an MCP definition, skill file or running process to see its source and evidence.</p></div>
        {/if}
      </aside>
    </div>
  {/if}
</section>

<style>
  .capabilities { height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--bg); color: var(--text); container-type: inline-size; }
  .workspace-head { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3); padding: var(--sp-4) var(--sp-5); }
  h2, h3, p { margin: 0; } h2 { display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-title); line-height: var(--lh-title); } h3 { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .eyebrow { display: block; color: var(--muted); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; margin-bottom: 4px; }
  .workspace-head p { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 4px; }
  .preview, .total, .scope { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); border: 1px solid var(--border); border-radius: var(--r-full); padding: 2px 7px; white-space: nowrap; }
  .preview { color: var(--tone-blue); background: var(--surface-2); }
  .evidence-strip { display: flex; flex-wrap: wrap; gap: var(--sp-2) var(--sp-5); padding: 8px var(--sp-5); border-block: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .evidence-strip span { display: inline-flex; align-items: center; gap: 6px; } .evidence-strip b { color: var(--text-2); font-weight: var(--fw-medium); }
  .workspace { display: grid; grid-template-columns: 205px minmax(0, 1fr) 288px; flex: 1; min-height: 0; }
  .agent-rail { padding: 12px 10px; border-right: 1px solid var(--border); overflow: auto; }
  .rail-title { display: flex; justify-content: space-between; padding: 0 6px 10px; font-size: var(--fs-label); color: var(--muted); } .rail-title span { font-variant-numeric: tabular-nums; }
  button { font: inherit; cursor: pointer; } button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .agent-choice { display: flex; width: 100%; align-items: flex-start; gap: 9px; padding: 10px 8px; margin-bottom: 5px; border: 1px solid transparent; border-radius: var(--r-md); background: transparent; color: var(--text-2); text-align: left; }
  .agent-choice.on { background: var(--row-selected); border-color: var(--border); color: var(--text); } .agent-choice:hover { background: var(--row-hover); }
  .avatar { display: grid; place-items: center; flex: none; width: 30px; height: 30px; border-radius: var(--r-md); color: var(--accent); background: var(--surface-2); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .agent-choice > span:last-child { display: grid; min-width: 0; gap: 3px; } .agent-choice b { font-size: var(--fs-body-sm); overflow-wrap: anywhere; } .agent-choice small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .rail-note { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); padding: 12px 6px; }
  .skill-description { margin-top: var(--sp-3); color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  .record-main .skill-purpose { color: var(--text-2); }
  .inventory-pane { min-width: 0; overflow: auto; padding: var(--sp-4); }
  .inventory-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: var(--sp-3); } .inventory-head p { margin-top: 3px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-3); margin-bottom: var(--sp-3); } .filter-selects { display: flex; flex-wrap: wrap; gap: var(--sp-2); } .filter-selects :global(.field) { min-width: 0; }
  .search-note { flex-basis: 100%; display: flex; flex-wrap: wrap; gap: 5px; color: var(--muted); font-size: var(--fs-caption); overflow-wrap: anywhere; } .search-note button, .usage-note button { border: 0; background: transparent; color: var(--accent); padding: 0; text-decoration: underline; text-underline-offset: 3px; }
  .relationship { border: 1px solid var(--border); border-radius: var(--r-lg); margin-bottom: var(--sp-3); overflow: hidden; background: var(--surface); }
  .diagram-caption { display: flex; align-items: center; gap: 6px; padding: 8px 12px; font-size: var(--fs-caption); color: var(--muted); border-bottom: 1px solid var(--border); }
  .diagram { display: grid; grid-template-columns: 1fr 1.15fr 1fr; gap: 16px; padding: 14px 12px; margin: 0; list-style: none; }
  .diagram li { min-width: 0; display: grid; justify-items: center; gap: 4px; text-align: center; position: relative; color: var(--muted); } .diagram li:not(:last-child)::after { content: "→"; position: absolute; right: -14px; top: 28px; color: var(--muted); }
  .diagram li > :global(svg) { color: var(--accent); } .diagram small, .diagram span { font-size: var(--fs-caption); line-height: var(--lh-caption); } .diagram b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); max-width: 100%; overflow-wrap: anywhere; } .diagram-focus { border-inline: 1px solid var(--border); padding-inline: 5px; }
  .records, .runtime ul { list-style: none; padding: 0; margin: 0; display: grid; gap: 7px; }
  .record { width: 100%; display: flex; align-items: center; gap: 10px; background: var(--surface); color: var(--text-2); border: 1px solid var(--border); border-radius: var(--r-md); padding: 11px 10px; text-align: left; }
  .record:hover, .runtime-row:hover { background: var(--row-hover); } .record.chosen, .runtime-row.chosen { border-color: var(--accent); background: var(--row-selected); }
  .record-icon { display: grid; place-items: center; flex: none; width: 30px; height: 32px; background: var(--surface-2); color: var(--accent); border-radius: var(--r-md); }
  .record-main { display: grid; gap: 3px; flex: 1; min-width: 0; } .record-main b { font-size: var(--fs-body); line-height: var(--lh-body); overflow-wrap: anywhere; } .record-main > span, .record-main small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow-wrap: anywhere; }
  .runtime { margin-top: var(--sp-5); border-top: 1px solid var(--border); padding-top: var(--sp-4); } .runtime h3 { display: flex; align-items: center; gap: 6px; } .runtime h3 span { margin-left: auto; color: var(--muted); font-size: var(--fs-caption); } .runtime header p, .runtime-empty { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); margin: 5px 0 10px; }
  .runtime-row { display: flex; width: 100%; gap: 8px; align-items: center; text-align: left; padding: 9px; color: var(--text-2); border: 1px solid var(--border); background: var(--surface); border-radius: var(--r-md); } .runtime-row > span { display: grid; gap: 3px; min-width: 0; flex: 1; } .runtime-row b { font-size: var(--fs-body-sm); overflow-wrap: anywhere; } .runtime-row small, .runtime-row code { color: var(--muted); font-size: var(--fs-caption); }
  .detail-pane { border-left: 1px solid var(--border); overflow: auto; min-width: 0; padding: var(--sp-5) var(--sp-4); background: var(--surface); }
  .detail-head h3 { display: flex; align-items: center; gap: 8px; margin-top: 7px; font-size: var(--fs-heading); line-height: var(--lh-heading); overflow-wrap: anywhere; } .detail-head p { margin-top: 9px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  dl { display: grid; gap: 15px; margin: var(--sp-5) 0; } dl > div { display: grid; gap: 5px; } dt { font-size: var(--fs-caption); color: var(--muted); } dd { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); overflow-wrap: anywhere; } .owner { display: block; margin-bottom: 3px; } .owner code { color: var(--muted); font-size: var(--fs-caption); } .path { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); user-select: text; }
  .usage-note { display: flex; align-items: flex-start; gap: 8px; padding: 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .usage-note b { color: var(--text-2); } .usage-note p { margin-top: 5px; } .usage-note button { display: inline-flex; align-items: center; gap: 5px; margin-top: 9px; font-size: var(--fs-caption); }
  .limits { margin-top: var(--sp-4); border-top: 1px solid var(--border); padding-top: 10px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } .limits summary { display: flex; align-items: center; gap: 6px; cursor: pointer; } .limits summary span { margin-left: auto; } .limits ul { display: grid; gap: 6px; padding-left: 18px; }
  .notice { padding: 8px var(--sp-4); display: flex; align-items: center; gap: var(--sp-3); }
  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 10px; color: var(--muted); padding: var(--sp-6); min-width: 0; } .empty h3 { color: var(--text-2); } .empty p { max-width: 520px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } .empty.compact { padding: var(--sp-5) var(--sp-3); } .empty-actions { display: flex; flex-wrap: wrap; gap: var(--sp-2); justify-content: center; }
  @container (max-width: 1100px) { .workspace { grid-template-columns: minmax(0, 1fr) 275px; grid-template-rows: auto minmax(0, 1fr); } .agent-rail { grid-column: 1 / -1; display: flex; gap: 7px; align-items: stretch; overflow-x: auto; border-right: 0; border-bottom: 1px solid var(--border); padding: 8px 12px; } .rail-title, .rail-note { display: none; } .agent-choice { flex: 0 0 160px; margin: 0; } }
  @container (max-width: 720px) { .workspace-head { flex-wrap: wrap; padding: var(--sp-3); } .evidence-strip { padding-inline: var(--sp-3); gap: 6px 12px; } .workspace { display: flex; flex-direction: column; overflow: auto; } .agent-rail { flex: none; overflow: auto; } .inventory-pane, .detail-pane { overflow: visible; flex: none; } .detail-pane { border-left: 0; border-top: 1px solid var(--border); } .inventory-list { max-height: 340px; overflow: auto; padding: 2px; } .record { gap: 7px; } .record .scope { display: none; } .diagram { padding-inline: 8px; gap: 12px; } .diagram li:not(:last-child)::after { right: -11px; } h2 { font-size: var(--fs-heading); line-height: var(--lh-heading); } .preview { font-size: var(--fs-caption); } }
</style>
