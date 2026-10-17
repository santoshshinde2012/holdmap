<script lang="ts">
  // Running agents as an aligned table: Agent | Projects | Ports | MCP | Skills | Access.
  // Rows expand to show their MCP servers inline. → / ← expand and collapse the focused row.
  import Icon from "../Icon.svelte";
  import McpServerList from "../agents/McpServerList.svelte";
  import { AGENT_COLORS, KIND_LABEL, monogram } from "../../lib/agents";
  import { configStatus } from "../../lib/agent-config";
  import { ACCESS_LABEL, type AgentRow } from "../../lib/home";

  let { rows, inventory = false, onopen, onentry, oninventory }: {
    rows: AgentRow[];
    inventory?: boolean;
    onopen: (id: string, section: "overview" | "capabilities") => void;
    onentry: (entryId: string) => void;
    oninventory?: () => void;
  } = $props();

  let expanded = $state<Record<string, boolean>>({});
  function toggle(id: string, on = !expanded[id]) { expanded = { ...expanded, [id]: on }; }
  function onkey(e: KeyboardEvent, id: string) {
    if (e.key === "ArrowRight") { e.preventDefault(); toggle(id, true); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); toggle(id, false); }
  }
</script>

<div class="table" role="table" aria-label="Running agents">
  <div class="head" role="row">
    <span role="columnheader">Agent</span><span role="columnheader">Projects</span><span role="columnheader">Ports</span>
    <span role="columnheader">MCP</span><span role="columnheader">Skills &amp; rules</span><span role="columnheader">Access</span>
  </div>
  {#each rows as row, i (row.agent.id)}
    {@const a = row.agent}
    {@const st = configStatus(a)}
    {@const open = !!expanded[a.id]}
    <div class="row" class:open role="row" style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}">
      <div class="cell who" role="cell">
        <button type="button" class="exp" aria-expanded={open} aria-controls="mcp-{a.id}" aria-label="{open ? 'Hide' : 'Show'} {a.name} MCP servers" onclick={() => toggle(a.id)}><Icon name="chevron" size={12} /></button>
        <span class="mono" aria-hidden="true">{monogram(a.name)}</span>
        <button type="button" class="name" data-home-nav onkeydown={(e) => onkey(e, a.id)} onclick={() => onopen(a.id, "overview")} aria-label="{a.name}, {KIND_LABEL[a.kind]}, {row.posture.text}">
          <span class="n">{a.name}</span><span class="kind"><span class="nw">{KIND_LABEL[a.kind]}</span> · <span class="nw">PID {a.pid}</span></span>
        </button>
      </div>
      <div class="cell projects" role="cell" data-label="Projects">
        {#if row.projects.length}
          {#each row.projects as p}<span class="proj"><Icon name="folder" size={12} />{p}</span>{/each}
          {#if row.moreProjects}<span class="faint">+{row.moreProjects} more</span>{/if}
        {:else}<span class="faint">No project folder</span>{/if}
      </div>
      <div class="cell ports" role="cell" data-label="Ports">
        {#each row.ports as p (p.entry_id)}
          <button type="button" class="chip" class:exposed={p.exposure === "all_interfaces"} onclick={() => onentry(p.entry_id)} aria-label="Port {p.port}{p.exposure === 'all_interfaces' ? ', open to the network' : ''}">:{p.port}</button>
        {:else}<span class="faint">None</span>{/each}
      </div>
      <div class="cell" role="cell" data-label="MCP">
        {#if st.kind === "read"}
          <button type="button" class="stat" onclick={() => toggle(a.id, true)} aria-label="MCP servers: {st.summary.running} running of {st.summary.mcp} configured">
            <b class:on={st.summary.running > 0}>{st.summary.running}</b><span class="of">/ {st.summary.mcp}</span> running
          </button>
          {#if st.summary.unpinned}<span class="warn-text">{st.summary.unpinned} unpinned</span>{/if}
        {:else if st.kind === "unsupported"}
          <span class="faint" title={st.reason}>Not supported</span>
        {:else if oninventory && !inventory}
          <button type="button" class="action" onclick={oninventory}>Read config</button>
        {:else}<span class="faint">Loading…</span>{/if}
      </div>
      <div class="cell" role="cell" data-label="Skills & rules">
        {#if st.kind === "read"}
          <button type="button" class="stat" onclick={() => onopen(a.id, "overview")} aria-label="{st.summary.skills} skills, {st.summary.instructions} instruction files, {st.summary.rules} permission rules">
            <b>{st.summary.skills + st.summary.instructions}</b> skills · <b>{st.summary.rules}</b> rules
          </button>
        {:else if st.kind === "unsupported"}
          <span class="faint" title={st.reason}>Not supported</span>
        {:else}<span class="faint">Not read</span>{/if}
      </div>
      <div class="cell access lv-{row.posture.level}" role="cell" data-label="Access">
        <span class="level">{ACCESS_LABEL[row.posture.level]}</span>
        <span class="why" data-full>{row.posture.text}</span>
      </div>
      {#if open}
        <div class="detail" id="mcp-{a.id}" role="cell">
          {#if st.kind === "read" && (a.capabilities?.mcp_servers.length ?? 0)}
            <McpServerList servers={a.capabilities!.mcp_servers} compact />
          {:else if st.kind === "read"}
            <p class="faint">No MCP servers in the inspected locations.</p>
          {:else if st.kind === "unsupported"}
            <p class="faint">{st.reason}</p>
          {:else}
            <p class="faint">MCP servers aren't read yet. {#if oninventory}<button type="button" class="action" onclick={oninventory}>Read config</button>{/if}</p>
          {/if}
          <button type="button" class="link" onclick={() => onopen(a.id, "overview")}>Open {a.name} <Icon name="chevron" size={11} /></button>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .table { --cols: minmax(190px, 1.35fr) minmax(130px, 1.1fr) minmax(96px, 0.8fr) minmax(104px, 0.75fr) minmax(112px, 0.8fr) minmax(170px, 1.35fr); container-type: inline-size; display: grid; }
  .head, .row { display: grid; grid-template-columns: var(--cols); column-gap: var(--sp-4); padding: 0 var(--sp-3); }
  .head { padding-block: 0 6px; border-bottom: 1px solid var(--border); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); }
  .head span:first-child { padding-left: 50px; }
  .row { padding-block: 8px; border-bottom: 1px solid var(--border); align-items: start; }
  .row:last-child { border-bottom: 0; }
  .row:hover, .row:focus-within { background: var(--row-hover); }
  .cell { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); padding-top: 3px; }
  button { font: inherit; color: inherit; cursor: pointer; }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .who { flex-wrap: nowrap; align-items: start; padding-top: 0; gap: 6px; }
  .exp { flex: none; width: 20px; height: 20px; margin-top: 3px; display: grid; place-items: center; border: 0; border-radius: var(--r-sm); background: none; color: var(--faint); }
  .exp :global(svg) { transition: transform 120ms ease; }
  .exp[aria-expanded="true"] :global(svg) { transform: rotate(90deg); }
  .exp:hover { background: var(--surface-2); color: var(--text); }
  .mono { width: 24px; height: 24px; flex: none; margin-top: 1px; display: grid; place-items: center; border-radius: 7px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); line-height: 1; font-weight: var(--fw-semibold); }
  .name { display: grid; min-width: 0; border: 0; padding: 0 2px; border-radius: var(--r-sm); background: none; text-align: left; }
  .n { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; }
  .kind, .faint, .of { color: var(--muted); }
  .nw { white-space: nowrap; }
  .proj { display: inline-flex; align-items: center; gap: 3px; overflow-wrap: anywhere; }
  .proj :global(svg) { color: var(--faint); flex: none; }
  .chip { border: 1px solid var(--border); background: var(--surface); padding: 0 6px; border-radius: var(--r-full); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .chip:hover, .stat:hover { border-color: var(--border-strong); color: var(--text); }
  .chip.exposed { border-color: color-mix(in srgb, var(--warn) 45%, transparent); color: var(--warn); background: var(--warn-soft); }
  .stat { display: inline-flex; align-items: baseline; gap: 3px; border: 1px solid transparent; background: none; padding: 0 4px; margin-left: -4px; border-radius: var(--r-sm); white-space: nowrap; }
  .stat b { font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; font-size: var(--fs-body-sm); }
  .stat b.on { color: var(--ok); }
  .warn-text { color: var(--warn); }
  .action { border: 1px solid var(--border-strong); background: var(--surface); padding: 0 8px; border-radius: var(--r-full); color: var(--accent); white-space: nowrap; }
  .action:hover { background: var(--accent-softer); }
  .access { display: grid; gap: 1px; }
  .level { font-weight: var(--fw-semibold); color: var(--text); }
  .lv-elevated .level { color: var(--danger); }
  .lv-restricted .level { color: var(--ok); }
  .why { color: var(--muted); overflow-wrap: anywhere; }
  .detail { grid-column: 1 / -1; margin: 8px 0 2px 50px; padding: 10px 12px; border-radius: var(--r-md); background: var(--surface-2); display: grid; gap: 8px; }
  .detail p { margin: 0; display: flex; gap: 8px; align-items: center; flex-wrap: wrap; font-size: var(--fs-caption); }
  .link { justify-self: start; display: inline-flex; align-items: center; gap: 2px; border: 0; background: none; padding: 0; color: var(--accent); font-size: var(--fs-caption); }
  @container (max-width: 820px) {
    .head { display: none; }
    .row { grid-template-columns: 1fr 1fr; row-gap: 6px; }
    .who { grid-column: 1 / -1; }
    .cell[data-label]::before { content: attr(data-label); flex-basis: 100%; color: var(--faint); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; }
    .detail { margin-left: 0; }
  }
  @container (max-width: 300px) { .row { grid-template-columns: 1fr; } }
</style>
