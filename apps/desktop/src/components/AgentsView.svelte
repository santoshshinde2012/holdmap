<script lang="ts">
  // The agents map: every AI coding agent and developer tool running here, with its folders,
  // the ports it serves, the services and hosts it talks to and how much access it has. A rail
  // of cards on the left and a column graph of their shared footprint on the right.
  import { SvelteFlow, Background, Controls, Panel, type NodeTypes, type EdgeTypes } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import AgentCard from "./agents/AgentCard.svelte";
  import AgentNode from "./agents/AgentNode.svelte";
  import FootNode from "./agents/FootNode.svelte";
  import HeaderNode from "./agents/HeaderNode.svelte";
  import FootEdge from "./agents/FootEdge.svelte";
  import FitOnChange from "./graph/FitOnChange.svelte";
  import AutoMiniMap from "./graph/AutoMiniMap.svelte";
  import Icon from "./Icon.svelte";
  import { FIT_PADDING } from "../lib/flow";
  import { agentColor, footprintGraph, layoutFootprint, signature, toFootFlow, type FootFlowEdge, type FootFlowNode, type FootNode as FNode, type Positions } from "../lib/agents";
  import type { AgentsReport, PortEntry } from "../lib/types";

  let {
    report,
    entries,
    selectedAgent,
    selectedEntry = null,
    dark,
    reduced,
    onselectagent,
    onentry,
    onreveal,
    oneditor,
    onstop,
    onstopall,
  }: {
    /** null while the first report loads. */
    report: AgentsReport | null;
    entries: PortEntry[];
    selectedAgent: string | null;
    /** The port open in the details pane, highlighted when it's on the map. */
    selectedEntry?: string | null;
    dark: boolean;
    reduced: boolean;
    onselectagent: (id: string | null) => void;
    onentry: (e: PortEntry) => void;
    onreveal: (agentId: string, path: string) => void;
    oneditor: (agentId: string, path: string) => void;
    onstop: (e: PortEntry) => void;
    onstopall: (agentId: string) => void;
  } = $props();

  const store = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
  const save = (k: string, v: string) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } };
  let recent = $state(store("pw.agents.recent") !== "0");
  $effect(() => save("pw.agents.recent", recent ? "1" : "0"));
  let hover = $state<string | null>(null);
  /** A folder / host picked on the map (agents and ports select through the app). */
  let picked = $state<string | null>(null);

  const nodeTypes: NodeTypes = { agent: AgentNode, foot: FootNode, header: HeaderNode } as unknown as NodeTypes;
  const edgeTypes: EdgeTypes = { foot: FootEdge } as unknown as EdgeTypes;

  const graph = $derived(report ? footprintGraph(report, { recent }) : null);
  const sig = $derived(graph ? signature(graph) : "");
  // Re-layout only when the structure changes, so refreshes don't make nodes jump.
  const cache: { sig: string; pos: Positions } = { sig: "", pos: new Map() };
  const positions = $derived.by(() => {
    if (!graph) return new Map() as Positions;
    if (sig !== cache.sig) {
      cache.pos = layoutFootprint(graph);
      cache.sig = sig;
    }
    return cache.pos;
  });
  const entryNode = $derived(selectedEntry && graph?.nodes.some((n) => n.id === `port:${selectedEntry}`) ? `port:${selectedEntry}` : null);
  const chosen = $derived(picked ?? entryNode ?? selectedAgent);
  const flow = $derived.by(() => {
    if (!graph) return { nodes: [] as FootFlowNode[], edges: [] as FootFlowEdge[] };
    return toFootFlow(graph, positions, { focus: hover ?? chosen, selectedId: chosen });
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
    const e = n.entryId ? entries.find((x) => x.id === n.entryId) : undefined;
    if (e) { picked = null; onentry(e); return; }
    picked = picked === n.id ? null : n.id;
  }
  const byId = (id: string | null | undefined) => (id ? graph?.nodes.find((n) => n.id === id) : undefined);
  // Enter / Space on a focused map node does what a click does.
  function onkeydown(ev: KeyboardEvent) {
    if (ev.key !== "Enter" && ev.key !== " ") return;
    const el = (ev.target as HTMLElement | null)?.closest?.(".svelte-flow__node") as HTMLElement | null;
    if (!el) return;
    ev.preventDefault();
    ev.stopPropagation();
    activate(byId(el.dataset.id));
  }

  const stats = $derived(report ? {
    agents: report.agents.length,
    folders: graph?.nodes.filter((n) => n.kind === "folder").length ?? 0,
    ports: graph?.nodes.filter((n) => n.kind === "port" || n.kind === "service").length ?? 0,
    hosts: report.agents.reduce((s, a) => s + a.links.filter((l) => l.kind === "remote").length + a.more_links, 0),
  } : null);
  const legend: [string, string, string][] = [
    ["agent", "Agent", "bot"],
    ["folder", "Folder", "folder"],
    ["port", "Port it serves", "server"],
    ["service", "Service it uses", "plug"],
    ["remote", "Remote host", "globe"],
  ];
  const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="agents" class:reduced {onkeydown}>
  {#if !report}
    <div class="empty" aria-busy="true"><Icon name="bot" size={28} /><p>Looking for agents…</p></div>
  {:else if report.agents.length === 0}
    <div class="empty">
      <Icon name="bot" size={28} />
      <p>No agents or developer tools running</p>
      <span>Start Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider, Docker Desktop or another tool: its folders, the ports and apps it started, connections and access show up here.</span>
    </div>
  {:else}
    <aside class="rail" aria-label="Agents">
      <header class="rail-head">
        <h2>Agents <span class="n">{report.agents.length}</span></h2>
        <span class="hint">↑↓ to switch · Reveal / Editor / Stop on a card · Enter on a map node</span>
      </header>
      <div class="cards">
        {#each report.agents as a (a.id)}
          <AgentCard agent={a} {report} {entries} color={graph ? agentColor(graph, a.id) : "var(--accent)"} selected={a.id === selectedAgent}
            onselect={() => { picked = null; onselectagent(a.id === selectedAgent ? null : a.id); }}
            {onentry}
            onreveal={(path) => onreveal(a.id, path)}
            oneditor={(path) => oneditor(a.id, path)}
            onstop={onstop}
            onstopall={() => onstopall(a.id)}
            now={report.taken_at_ms || Date.now()} />
        {/each}
      </div>
      <details class="limits">
        <summary><Icon name="info" size={12} />What this can and can't see</summary>
        <ul>{#each report.limits as l}<li>{l}</li>{/each}</ul>
      </details>
    </aside>
    <div class="map">
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
        <Panel position="top-left">
          <div class="bar" role="toolbar" aria-label="Agents map options">
            <button class="tog" aria-pressed={recent} class:on={recent} onclick={() => (recent = !recent)} title="Show projects the agent opened recently (from its own project list), not only where it works now"><Icon name="history" size={12} />Recent projects</button>
          </div>
        </Panel>
        <Panel position="top-right">
          {#if stats}
            <div class="stats" aria-live="polite">
              <span><b>{stats.agents}</b> {stats.agents === 1 ? "agent" : "agents"}</span>
              <span><b>{stats.folders}</b> {stats.folders === 1 ? "folder" : "folders"}</span>
              <span class="opt"><b>{stats.ports}</b> ports &amp; services</span>
              <span class="opt">{plural(stats.hosts, "remote link")}</span>
            </div>
          {/if}
          <div class="legend" aria-hidden="true">
            {#each legend as [k, l, icon]}<span class="lg lg-{k}"><Icon name={icon} size={11} />{l}</span>{/each}
            <span class="lg"><svg width="22" height="6"><line x1="0" y1="3" x2="22" y2="3" stroke="currentColor" stroke-width="1.6" stroke-dasharray="4 3" /></svg>recent / remote / inferred</span>
          </div>
        </Panel>
      </SvelteFlow>
    </div>
  {/if}
</div>

<style>
  .agents { position: relative; height: 100%; min-height: 0; display: grid; grid-template-columns: minmax(300px, 340px) 1fr; background: var(--bg); container-type: inline-size; }
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
  .map :global(.svelte-flow) { --xy-background-color: var(--bg); --xy-controls-button-background-color: var(--surface); --xy-controls-button-color: var(--text-2); --xy-controls-button-border-color: var(--border); --xy-minimap-background-color: var(--surface); }
  .map :global(.svelte-flow__node-agent), .map :global(.svelte-flow__node-foot), .map :global(.svelte-flow__node-header) { padding: 0; border: 0; background: transparent; box-shadow: none; width: auto; border-radius: 14px; }
  .map :global(.svelte-flow__node:focus-visible) { outline: 2px solid var(--accent); outline-offset: 3px; }
  .map :global(.svelte-flow__node-header) { pointer-events: none; }
  .map :global(.svelte-flow__controls) { box-shadow: var(--shadow-sm); border-radius: 10px; overflow: hidden; border: 1px solid var(--border); }
  .map :global(.svelte-flow__minimap) { border-radius: 12px; overflow: hidden; border: 1px solid var(--border); box-shadow: var(--shadow-sm); opacity: 0.72; }
  .map :global(.svelte-flow__background) { --xy-background-pattern-color: var(--border); }
  .bar { display: flex; gap: 6px; align-items: center; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: 11px; box-shadow: var(--shadow-sm); }
  .tog { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border: 0; border-radius: 6px; background: transparent; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .tog.on { color: var(--accent); background: var(--accent-soft); }
  .stats { display: flex; gap: 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; justify-content: flex-end; }
  .stats b { color: var(--text); }
  .legend { display: flex; flex-wrap: wrap; gap: 4px 10px; margin-top: 6px; justify-content: flex-end; max-width: 520px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lg { display: inline-flex; align-items: center; gap: 4px; }
  .lg-agent { color: var(--accent); }
  .lg-folder :global(svg) { color: var(--tone-amber); }
  .lg-port :global(svg) { color: var(--tone-green); }
  .lg-service :global(svg) { color: var(--tone-violet); }
  .lg-remote :global(svg) { color: var(--tone-blue); }
  .empty { grid-column: 1 / -1; height: 100%; display: grid; place-content: center; justify-items: center; gap: 6px; color: var(--muted); text-align: center; }
  .empty p { margin: 6px 0 0; color: var(--text); font-weight: var(--fw-semibold); }
  .empty span { font-size: var(--fs-body); line-height: var(--lh-body); max-width: 380px; }
  @container (max-width: 1100px) { .stats .opt { display: none; } }
  @container (max-width: 860px) {
    .agents { grid-template-columns: 1fr; grid-template-rows: minmax(0, 42%) 1fr; }
    .rail { border-right: 0; border-bottom: 1px solid var(--border); }
    .stats, .legend { display: none; }
  }
  .reduced :global(*) { animation: none !important; transition: none !important; }
</style>
