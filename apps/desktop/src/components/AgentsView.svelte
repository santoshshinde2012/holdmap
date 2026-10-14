<script lang="ts">
  // The agents map: every AI coding agent and developer tool running here, with C4-style zoom
  // over the same observed report (this computer, shared footprint, one source tree). A rail of
  // cards on the left and a column graph of connected nodes on the right.
  import { SvelteFlow, Background, Controls, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import AgentCard from "./agents/AgentCard.svelte";
  import AgentNode from "./agents/AgentNode.svelte";
  import FootNode from "./agents/FootNode.svelte";
  import HeaderNode from "./agents/HeaderNode.svelte";
  import FootEdge from "./agents/FootEdge.svelte";
  import FitOnChange from "./graph/FitOnChange.svelte";
  import AutoMiniMap from "./graph/AutoMiniMap.svelte";
  import Icon from "./Icon.svelte";
  import AgentFlowView from "./agents/flow/AgentFlowView.svelte";
  import FolderActions from "./FolderActions.svelte";
  import type { AgentFlowProps } from "../lib/agent-flow/model";
  import { FIT_PADDING } from "../lib/flow";
  import { agentColor, filterAgents, layoutPicture, pictureGraph, signature, toFootFlow, type FootFlowEdge, type FootFlowNode, type FootNode as FNode, type Picture, type Positions } from "../lib/agents";
  import type { AgentsReport, PortEntry } from "../lib/types";
  import { prefGet, prefGetBoolean, prefSet } from "../lib/prefs";
  import { untrack } from "svelte";
  import { currentFolderOwner, preferredFolderOwner, resolveFolderTarget, type FolderTarget } from "../lib/folders";
  import { isComposingKey } from "../lib/keyboard";
  import { ActionDock, ActionPill } from "./ui";

  let {
    report,
    query = "",
    entries,
    selectedAgent,
    selectedEntry = null,
    dark,
    reduced,
    section = $bindable("overview"),
    flowSource = $bindable("connections"),
    picture = $bindable<Picture>("context"),
    projectReady = $bindable(false),
    projectCaption = $bindable(""),
    pictureChrome = true,
    onselectagent,
    onentry,
    onreveal,
    oneditor,
    onstop,
    onstopall,
    platform,
    ...flowProps
  }: {
    /** null while the first report loads. */
    report: AgentsReport | null;
    query?: string;
    entries: PortEntry[];
    selectedAgent: string | null;
    /** The port open in the details pane, highlighted when it's on the map. */
    selectedEntry?: string | null;
    dark: boolean;
    reduced: boolean;
    section?: "overview" | "flow";
    flowSource?: "connections" | "calls";
    picture?: Picture;
    projectReady?: boolean;
    projectCaption?: string;
    /** When false the Computer / Footprint / Project dock lives on the app toolbar. */
    pictureChrome?: boolean;
    onselectagent: (id: string | null) => void;
    onentry: (e: PortEntry) => void;
    onreveal: (agentId: string, path: string) => void | Promise<void | boolean>;
    oneditor: (agentId: string, path: string) => void | Promise<void | boolean>;
    onstop: (e: PortEntry, agentId: string) => void;
    onstopall: (agentId: string) => void;
    /** Local computer platform; remote-view paths still use the native local action boundary. */
    platform?: string;
  } & AgentFlowProps = $props();

  $effect(() => { if (section === "overview") untrack(() => flowProps.onflowmodechange?.("overview")); });

  let recent = $state(prefGet("agents.recent") !== "0");
  $effect(() => prefSet("agents.recent", recent ? "1" : "0"));
  $effect(() => prefSet("agents.picture", picture));
  /** Source tree shown in the Project picture; set by picking a folder. Not persisted (paths change). */
  let projectPath = $state<string | null>(null);
  $effect(() => { projectReady = !!projectPath; });
  let rail = $state(prefGetBoolean("agents.rail", true));
  $effect(() => prefSet("agents.rail", rail ? "1" : "0"));
  let widgets = $state(prefGetBoolean("agents.widgets", true));
  $effect(() => prefSet("agents.widgets", widgets ? "1" : "0"));
  let hover = $state<string | null>(null);
  /** A folder / host picked on the map (agents and ports select through the app). */
  let picked = $state<string | null>(null);
  let folderTarget = $state.raw<FolderTarget | null>(null);
  let folderOwnerName = $state("");
  const folderOwner = $derived(currentFolderOwner(report, folderTarget));

  function openFolder(path: string, preferredAgentId = selectedAgent) {
    const target = resolveFolderTarget(report, path, preferredAgentId);
    if (!target) return;
    folderOwnerName = currentFolderOwner(report, target)?.name ?? "Agent";
    folderTarget = target;
  }

  const nodeTypes: NodeTypes = { agent: AgentNode, foot: FootNode, header: HeaderNode } as unknown as NodeTypes;
  const edgeTypes: EdgeTypes = { foot: FootEdge } as unknown as EdgeTypes;

  const visibleAgents = $derived(report ? filterAgents(report.agents, query) : []);
  const agentIds = $derived(report?.agents.map((a) => a.id) ?? []);
  const graph = $derived(report ? pictureGraph({ ...report, agents: visibleAgents }, picture, { recent, folder: projectPath }) : null);
  const sig = $derived(graph ? signature(graph) : "");
  // Re-layout only when the structure changes, so refreshes don't make nodes jump.
  const cache: { sig: string; pos: Positions } = { sig: "", pos: new Map() };
  const positions = $derived.by(() => {
    if (!graph) return new Map() as Positions;
    if (sig !== cache.sig) {
      cache.pos = layoutPicture(graph);
      cache.sig = sig;
    }
    return cache.pos;
  });
  const projectLabel = $derived(graph?.nodes.find((n) => n.kind === "folder" && n.path === projectPath)?.label ?? projectPath?.split(/[/\\]/).pop() ?? "this project");
  $effect(() => { projectCaption = projectPath ? projectLabel : ""; });
  const entryNode = $derived(selectedEntry && graph?.nodes.some((n) => n.id === `port:${selectedEntry}`) ? `port:${selectedEntry}` : null);
  const chosen = $derived([picked, entryNode, selectedAgent].find((id) => id && graph?.nodes.some((n) => n.id === id)) ?? null);
  const flow = $derived.by(() => {
    if (!graph) return { nodes: [] as FootFlowNode[], edges: [] as FootFlowEdge[] };
    return toFootFlow(graph, positions, { focus: graph.nodes.some((n) => n.id === hover) ? hover : chosen, selectedId: chosen, agentIds });
  });
  let nodes = $state.raw<FootFlowNode[]>([]);
  let edges = $state.raw<FootFlowEdge[]>([]);
  $effect(() => { nodes = flow.nodes; edges = flow.edges; });

  function activate(n: FNode | undefined) {
    if (!n || n.kind === "header") return;
    if (n.kind === "agent") {
      picked = null;
      onselectagent(n.id);
      return;
    }
    if (n.kind === "tool") { picked = n.id; onselectagent(n.owners[0] ?? null); return; }
    if (n.kind === "machine") { picked = n.id; return; }
    if (n.kind === "folder" && n.path) {
      const again = picked === n.id || (picture === "project" && projectPath === n.path);
      projectPath = n.path;
      picked = n.id;
      if (picture !== "project") { picture = "project"; return; }
      if (again) openFolder(n.path, preferredFolderOwner(n.owners, selectedAgent));
      return;
    }
    const e = n.entryId ? entries.find((x) => x.id === n.entryId) : undefined;
    if (e) { picked = null; onentry(e); return; }
    picked = picked === n.id ? null : n.id;
  }
  const byId = (id: string | null | undefined) => (id ? graph?.nodes.find((n) => n.id === id) : undefined);
  // Enter / Space on a focused map node does what a click does.
  function onkeydown(ev: KeyboardEvent) {
    if (isComposingKey(ev) || ev.repeat || ev.altKey || ev.ctrlKey || ev.metaKey) return;
    if (ev.key !== "Enter" && ev.key !== " ") return;
    const el = (ev.target as HTMLElement | null)?.closest?.(".svelte-flow__node") as HTMLElement | null;
    if (!el) return;
    ev.preventDefault();
    ev.stopPropagation();
    activate(byId(el.dataset.id));
  }

  const stats = $derived(report ? {
    agents: graph?.nodes.filter((n) => n.kind === "agent").length ?? visibleAgents.length,
    folders: graph?.nodes.filter((n) => n.kind === "folder").length ?? 0,
    ports: graph?.nodes.filter((n) => n.kind === "port" || n.kind === "service").length ?? 0,
    hosts: graph?.nodes.filter((n) => n.kind === "remote").length ?? 0,
    omittedLinks: picture === "footprint" ? visibleAgents.reduce((s, a) => s + a.more_links, 0) : 0,
  } : null);
  const legend = $derived(
    picture === "context"
      ? [["machine", "This computer", "monitor"], ["agent", "Agent", "bot"], ["remote", "Remote host", "globe"]] as [string, string, string][]
      : [
          ["agent", "Agent", "bot"],
          ["folder", picture === "project" ? "This project" : "Folder", "folder"],
          ["tool", "MCP server", "plug"],
          ["port", "Port it serves", "server"],
          ["service", "Service it uses", "plug"],
          ["remote", "Remote host", "globe"],
        ] as [string, string, string][],
  );
  const pictureHint = $derived(
    picture === "context"
      ? "Context — this computer, the agents on it, and systems they talk to. Open Footprint for folders and ports."
      : picture === "project"
        ? `Components in ${projectLabel} — click the folder again for Reveal / Editor.`
        : "Containers — click a folder to zoom into that source tree.",
  );
  const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;
  const showMap = $derived(!!visibleAgents.length && (picture !== "project" || (graph?.nodes.length ?? 0) > 0));
  const projectEmpty = $derived(picture === "project" && !(graph?.nodes.length));
</script>

<div class="agents-root" class:reduced>
  {#if section === "flow"}
    <div class="flow-body"><AgentFlowView {query} {dark} {reduced} {...flowProps} bind:mode={flowSource} sourceChrome={false} /></div>
  {:else}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="agents" class:rail-off={!rail} {onkeydown}>
  {#if !report}
    <div class="empty" aria-busy="true"><Icon name="bot" size={28} /><p>Looking for agents…</p></div>
  {:else if report.agents.length === 0}
    <div class="empty">
      <Icon name="bot" size={28} />
      <p>No agents or developer tools running</p>
      <span>Start Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider, Docker Desktop or another tool: its folders, the ports and apps it started, connections and access show up here.</span>
      {#if report.limits.length}
        <details class="limits" open>
          <summary><Icon name="info" size={12} />What this can and can't see</summary>
          <ul>{#each report.limits as l}<li>{l}</li>{/each}</ul>
        </details>
      {/if}
    </div>
  {:else}
    {#if rail}
    <aside class="rail" aria-label="Agents">
      <header class="rail-head">
        <h2>Agents <span class="n">{visibleAgents.length}{#if visibleAgents.length !== report.agents.length}{" / "}{report.agents.length}{/if}</span></h2>
        <span class="hint">↑↓ to switch · click a folder to zoom into that project · Enter on a map node</span>
      </header>
      <div class="cards" id="agent-list">
        {#each visibleAgents as a (a.id)}
          <AgentCard agent={a} {report} {entries} color={graph ? agentColor(graph, a.id, agentIds) : "var(--accent)"} selected={a.id === selectedAgent} onselect={() => { picked = null; onselectagent(a.id === selectedAgent ? null : a.id); }} onnavigate={(id) => { picked = null; onselectagent(id); }}
            {onentry} onreveal={(path) => onreveal(a.id, path)} oneditor={(path) => oneditor(a.id, path)}
            onfolder={(path) => { projectPath = path; openFolder(path, a.id); }}
            onstop={(entry) => onstop(entry, a.id)} onstopall={() => onstopall(a.id)} now={report.taken_at_ms || Date.now()} />
        {:else}
          <div class="no-matches"><Icon name="search" size={20} /><p>No agents match this search</p><span>Search by name, tool, folder, port or process ID.</span></div>
        {/each}
      </div>
      <details class="limits">
        <summary><Icon name="info" size={12} />What this can and can't see</summary>
        <ul>{#each report.limits as l}<li>{l}</li>{/each}</ul>
      </details>
    </aside>
    {/if}
    <div class="map">
      {#if visibleAgents.length || projectPath}
      <div class="chrome-host">
        <div class="chrome">
          <div class="bar" role="toolbar" aria-label="Agents map options">
            {#if pictureChrome}
              <ActionDock size="compact" hinge={false} animate={false} label="Map picture" bind:value={picture} options={[
                { value: "context", label: "Computer", icon: "monitor", title: "Context: this computer, the agents on it, and systems they talk to" },
                { value: "footprint", label: "Footprint", icon: "layers", title: "Containers: folders, tools, ports and hosts across every agent" },
                { value: "project", label: "Project", icon: "folder", title: projectPath ? `Components in ${projectLabel}` : "Select a folder on Footprint to see that source tree", disabled: !projectPath },
              ]} />
            {/if}
            <ActionPill size="compact" toggle icon="sidebar" label={rail ? "Hide cards" : "Show cards"} pressed={rail} title={rail ? "Hide agent cards so the map uses the full width" : "Show agent cards"} onclick={() => (rail = !rail)} />
            {#if picture !== "context"}<ActionPill size="compact" toggle icon="history" label="Recent projects" pressed={recent} title="Show projects the agent opened recently (from its own project list), not only where it works now" onclick={() => (recent = !recent)} />{/if}
            <ActionPill size="compact" toggle icon="layers" label="Legend" pressed={widgets} title="Show or hide the legend and counts" onclick={() => (widgets = !widgets)} />
          </div>
          {#if widgets && stats && !projectEmpty}
            <div class="stats" aria-live="polite">
              <span class="sr-only">{pictureHint}</span>
              <span><b>{stats.agents}</b> {stats.agents === 1 ? "agent" : "agents"}</span>
              {#if picture !== "context"}<span><b>{stats.folders}</b> {stats.folders === 1 ? "folder" : "folders"}</span>{/if}
              {#if picture !== "context"}<span class="opt"><b>{stats.ports}</b> ports &amp; services</span>{/if}
              <span class="opt">{plural(stats.hosts, "remote host")}</span>
              {#if stats.omittedLinks}<span class="opt">+{stats.omittedLinks} unlisted links</span>{/if}
            </div>
          {/if}
          {#if widgets && !projectEmpty}
            <div class="legend" aria-hidden="true">
              {#each legend as [k, l, icon]}<span class="lg lg-{k}"><Icon name={icon} size={11} />{l}</span>{/each}
              <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>recent / remote / inferred</span>
            </div>
          {/if}
        </div>
      </div>
      {/if}
      {#if projectEmpty}
        <div class="empty">
          <Icon name="folder" size={28} />
          <p>No agents in this project match</p>
          <span>Open Footprint and click a working folder, or clear search. Only observed agents, ports, tools and hosts for that path are shown.</span>
        </div>
      {:else if showMap}
      <SvelteFlow
        bind:nodes
        bind:edges
        {nodeTypes}
        {edgeTypes}
        colorMode={dark ? "dark" : "light"}
        fitView
        fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.1 }}
        minZoom={0.2}
        maxZoom={2}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={true}
        proOptions={{ hideAttribution: true }}
        onnodeclick={({ node }) => activate((node.data as FootFlowNode["data"]).node)}
        onnodepointerenter={({ node }) => { if (node.type !== "header") hover = node.id; }}
        onnodepointerleave={() => (hover = null)}
        onpaneclick={() => { picked = null; }}
      >
        <FitOnChange token={sig} focus={null} {reduced} />
        <Background gap={22} size={1.2} />
        <Controls showLock={false} position="bottom-left" fitViewOptions={{ padding: FIT_PADDING, maxZoom: 1.1, duration: reduced ? 0 : 320 }} />
        <AutoMiniMap {dark} />
      </SvelteFlow>
      {/if}
    </div>
  {/if}
</div>
  {/if}
</div>

{#if folderTarget}
  <FolderActions target={folderTarget} ownerName={folderOwnerName} available={!!folderOwner} {platform} demo={flowProps.demo ?? false} {onreveal} {oneditor} onclose={() => (folderTarget = null)} />
{/if}

<style>
  .agents-root { height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--bg); }
  .agents.rail-off { grid-template-columns: 1fr; }
  .flow-body { flex: 1; min-height: 0; }
  .agents { position: relative; flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(300px, 340px) 1fr; background: var(--bg); container-type: inline-size; }
  .rail { min-height: 0; display: flex; flex-direction: column; border-right: 1px solid var(--border); background: var(--bg); }
  .rail-head { padding: 12px 14px 8px; display: grid; gap: 2px; }
  h2 { margin: 0; display: flex; align-items: center; gap: 8px; font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); }
  .n { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--muted); background: var(--surface-2); padding: 0 7px; border-radius: 999px; font-variant-numeric: tabular-nums; }
  .hint { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .cards { flex: 1; min-height: 0; overflow: auto; padding: 4px 12px 12px; display: grid; align-content: start; gap: 8px; }
  .limits { border-top: 1px solid var(--border); padding: 8px 14px 10px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .limits summary { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; color: var(--text-2); }
  .limits ul { margin: 6px 0 0; padding-left: 16px; display: grid; gap: 3px; max-height: 160px; overflow: auto; }
  .map { position: relative; min-width: 0; min-height: 0; }
  .chrome-host { position: absolute; top: 10px; left: 10px; z-index: 5; pointer-events: none; }
  .chrome-host .bar, .chrome-host .stats, .chrome-host .legend { pointer-events: auto; }
  .map :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .map :global(.svelte-flow__node-agent), .map :global(.svelte-flow__node-foot), .map :global(.svelte-flow__node-header) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; border-radius: 14px; }
  .map :global(.svelte-flow__node:focus-visible) { outline: 2px solid var(--accent); outline-offset: 3px; }
  .map :global(.svelte-flow__node-header) { pointer-events: none; }
  .map :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  .map :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); opacity: 0.72; }
  .map :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .chrome { display: flex; flex-direction: column; align-items: flex-start; gap: 6px; max-width: min(100%, 560px); }
  .bar { display: flex; gap: 6px; align-items: center; width: max-content; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: 11px; box-shadow: var(--shadow-sm); }
  .stats { display: flex; gap: 10px; width: max-content; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .stats b { color: var(--text); }
  .legend { display: flex; flex-wrap: wrap; gap: 4px 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lg { display: inline-flex; align-items: center; gap: 4px; }
  .lg-agent { color: var(--accent); }
  .lg-machine { color: var(--tone-blue); }
  .lg-folder :global(svg) { color: var(--tone-amber); }
  .lg-port :global(svg) { color: var(--tone-green); }
  .lg-service :global(svg) { color: var(--tone-violet); }
  .lg-tool :global(svg) { color: var(--tone-violet); }
  .lg-remote :global(svg) { color: var(--tone-blue); }
  .empty { grid-column: 1 / -1; height: 100%; display: grid; place-content: center; justify-items: center; gap: 6px; color: var(--muted); text-align: center; }
  .empty p { margin: 6px 0 0; color: var(--text); font-weight: var(--fw-semibold); }
  .empty span { font-size: var(--fs-body); line-height: var(--lh-body); max-width: 380px; }
  .empty .limits { margin-top: 12px; max-width: 460px; text-align: left; }
  .no-matches { padding: 20px 4px; display: grid; gap: 6px; justify-items: center; text-align: center; color: var(--muted); }
  .no-matches p { margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); color: var(--text); }
  .no-matches span { font-size: var(--fs-caption); line-height: var(--lh-caption); }
  @container (max-width: 1100px) { .stats .opt { display: none; } }
  @container (max-width: 860px) {
    .agents:not(.rail-off) { grid-template-columns: 1fr; grid-template-rows: minmax(0, 42%) 1fr; }
    .rail { border-right: 0; border-bottom: 1px solid var(--border); }
    .stats, .legend { display: none; }
  }
  .reduced :global(*) { animation: none !important; transition: none !important; }
</style>
