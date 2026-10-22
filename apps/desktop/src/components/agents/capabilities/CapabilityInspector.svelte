<script lang="ts">
  import Icon from "../../Icon.svelte";
  import { Button, EvidenceChip } from "../../ui";
  import { capabilityChip } from "../../../lib/evidence";
  import { enabledLabel, scopeLabel, type CapabilitySelection } from "../../../lib/agent-capabilities";
  import { EVIDENCE_LABEL } from "../../../lib/agents";
  import { humanBytes } from "../../../lib/format";
  import { isComposingKey } from "../../../lib/keyboard";
  import { nextIndex } from "../../../lib/roving";
  import type { AgentMcpServer, AgentSkill } from "../../../lib/types";
  // Scan with… loads on first open: it pulls in the analyzer store and API.
  const scanWith = import("../../analyzers/ScanWith.svelte");

  let { item, takenAt, onselectagent, onrecordedcalls, oncoverage, onsettings, onsecurity }: {
    item: CapabilitySelection; takenAt: number | null; onselectagent: (id: string) => void;
    onrecordedcalls?: () => void; oncoverage: () => void;
    /** Analyzer settings (consent lives there) and the Security tab, for "Scan with…". */
    onsettings?: () => void; onsecurity?: () => void;
  } = $props();
  type DetailTab = "purpose" | "source" | "evidence" | "activity";
  const tabs: { id: DetailTab; label: string }[] = [{ id: "purpose", label: "Purpose" }, { id: "source", label: "Source" }, { id: "evidence", label: "Evidence" }, { id: "activity", label: "Activity" }];
  const prefix = $props.id();
  let tab = $state<DetailTab>("purpose");
  let heading: HTMLHeadingElement | undefined = $state();
  let tabButtons: HTMLButtonElement[] = $state([]);
  const skill = $derived(item.kind === "skill" ? item.definition as AgentSkill : null);
  const mcp = $derived(item.kind === "mcp" ? item.definition as AgentMcpServer : null);
  const chip = $derived(capabilityChip(item.kind));
  const stamp = $derived(takenAt !== null && Number.isFinite(takenAt) && takenAt > 0 ? new Date(takenAt).toLocaleString() : null);
  export function focusHeading() { heading?.focus({ preventScroll: true }); }
  function key(event: KeyboardEvent) {
    if (isComposingKey(event) || event.altKey || event.ctrlKey || event.metaKey) return;
    const index = nextIndex(tabs.findIndex(item => item.id === tab), event.key, tabs.length);
    if (index !== null) { event.preventDefault(); tab = tabs[index].id; tabButtons[index]?.focus(); }
  }
</script>

<div class="inspector">
  <span class="eyebrow"><EvidenceChip {...chip} /></span>
  <h3 tabindex="-1" bind:this={heading}><span class="symbol"><Icon name={item.kind === "skill" ? "sparkles" : item.kind === "mcp" ? "plug" : "cpu"} size={20} /></span>{item.name}</h3>
  <div class="tabs" role="tablist" tabindex="-1" aria-label="Capability inspection" onkeydown={key}>
    {#each tabs as entry, index (entry.id)}
      <button id={`${prefix}-${entry.id}`} role="tab" aria-selected={tab === entry.id} aria-controls={`${prefix}-panel`} tabindex={tab === entry.id ? 0 : -1} bind:this={tabButtons[index]} onclick={() => tab = entry.id}>{entry.label}</button>
    {/each}
  </div>
  <div class="panel" id={`${prefix}-panel`} role="tabpanel" aria-labelledby={`${prefix}-${tab}`} tabindex="0">
    {#if tab === "purpose"}
      <h4>{skill ? "Declared purpose" : "What this is"}</h4>
      <p>{skill ? skill.description || "No description in the skill header." : mcp ? "An MCP server defined in this agent's config. Its tools aren't reported." : "A process that looks like an MCP server. Not linked to any config entry."}</p>
      <h4>Why it's listed</h4>
      <p>{skill ? "This SKILL.md is in a source the agent reads. The description is the author's claim." : mcp ? "Found in a supported config file, with its name, scope and transport." : "Process evidence only. A similar name doesn't make it the configured server."}</p>
    {:else if tab === "source"}
      <h4>Source &amp; context</h4>
      <dl>
        {#if item.kind === "running"}
          <div><dt>Process ID</dt><dd><code>{item.tool.pid}</code></dd></div>
          {#if item.tool.cwd}<div><dt>Working folder</dt><dd class="path">{item.tool.cwd}</dd></div>{/if}
          <div><dt>Listening ports</dt><dd>{item.tool.ports.length ? item.tool.ports.map(port => ":" + port).join(", ") : "None"}</dd></div>
          <div><dt>Resources</dt><dd>{item.tool.cpu_percent.toFixed(1)}% CPU · {humanBytes(item.tool.memory_bytes)}</dd></div>
        {:else}
          <div><dt>Source file</dt><dd class="path">{item.sourcePath}</dd></div>
          <div><dt>Scope</dt><dd>{scopeLabel[item.scope]}</dd></div>
          {#if item.projectPath}<div><dt>Project relationship</dt><dd class="path">{item.projectPath}</dd></div>{/if}
          {#if skill && item.name !== item.definition.name}<div><dt>Skill folder name</dt><dd>{item.definition.name}</dd></div>{/if}
          {#if mcp}<div><dt>Transport declaration</dt><dd>{mcp.transport === "unknown" ? "Not reported" : mcp.transport.toUpperCase()}</dd></div><div><dt>Configuration enablement</dt><dd>{enabledLabel(item)}</dd></div>{/if}
        {/if}
        <div><dt>Associated agents</dt><dd>{#each item.owners as owner (owner.id)}<button class="owner" onclick={() => onselectagent(owner.id)}>{owner.name} <code>PID {owner.pid}</code><Icon name="chevron" size={12} /></button>{/each}</dd></div>
      </dl>
    {:else if tab === "evidence"}
      <h4>Why these are connected</h4>
      <p>{item.kind === "running" ? "Not matched to a config entry or a call." : item.kind === "skill" ? "Skill file found where these agents look for skills." : "Found in these agents' config. Configured isn't the same as connected or used."}</p>
      <dl>
        <div><dt>Evidence</dt><dd>{item.kind === "running" ? EVIDENCE_LABEL[item.tool.evidence] + " process classification" : EVIDENCE_LABEL[item.definition.evidence] + " file metadata"}</dd></div>
        <div><dt>Report snapshot</dt><dd>{stamp ?? "Time not reported"}</dd></div>
        <div><dt>{skill ? "Loading & activation" : mcp ? "Connection & catalog" : "Connection & invocation"}</dt><dd>Not confirmed</dd></div>
      </dl>
      <Button size="sm" variant="ghost" icon="info" onclick={oncoverage}>View coverage</Button>
    {:else}
      <h4>{skill ? "Activation unknown" : "Usage unknown"}</h4>
      <p>No recorded calls are linked to this item. No record doesn't mean unused.</p>
      {#if onrecordedcalls}<Button size="sm" icon="activity" onclick={onrecordedcalls}>Open recorded calls</Button>{:else}<p class="muted">Recorded calls are not available in this view.</p>{/if}
    {/if}
  </div>
  {#if item.kind !== "running"}{#await scanWith then { default: ScanWith }}<ScanWith kind={item.kind} subject={item.kind === "skill" ? item.definition.id : null} {onsettings} {onsecurity} />{/await}{/if}
</div>

<style>
  .inspector { padding: 20px 18px; }
  h3, h4, p { margin: 0; }
  .eyebrow { display: block; margin-bottom: 12px; }
  h3 { display: flex; align-items: center; gap: 10px; font-size: var(--fs-heading); line-height: var(--lh-heading); overflow-wrap: anywhere; outline: none; }
  .symbol { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border-radius: 11px; background: var(--accent-soft); color: var(--accent); }
  .tabs { display: flex; gap: 3px; margin: 22px -5px 18px; padding-bottom: 9px; border-bottom: 1px solid var(--border); }
  .tabs button { flex: 1; min-width: 0; padding: 7px 3px; border: 0; border-radius: 7px; background: transparent; color: var(--muted); font: inherit; font-size: var(--fs-caption); cursor: pointer; }
  .tabs button[aria-selected="true"] { background: var(--accent-soft); color: var(--accent); }
  button:focus-visible, .panel:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .panel { border-radius: 4px; }
  h4 { margin-top: 20px; margin-bottom: 9px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); } h4:first-child { margin-top: 0; }
  p, dd { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; color: var(--text-2); }
  p + p { margin-top: 14px; }
  .muted { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dl { display: grid; gap: 17px; margin: 15px 0; } dl > div { display: grid; gap: 5px; }
  dt { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); } dd { margin: 0; }
  code, .path { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .owner { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; width: 100%; border: 0; padding: 5px 0; text-align: left; color: var(--text-2); background: transparent; font: inherit; font-size: var(--fs-body-sm); cursor: pointer; }
  .owner code { color: var(--muted); } .owner :global(svg) { margin-left: auto; }
</style>
