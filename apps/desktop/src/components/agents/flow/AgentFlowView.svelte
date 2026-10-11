<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import SegmentedControl from "../../ui/SegmentedControl.svelte";
  import Icon from "../../Icon.svelte";
  import FlowCanvas from "./FlowCanvas.svelte";
  import FlowInspector from "./FlowInspector.svelte";
  import ActivityTimeline from "./ActivityTimeline.svelte";
  import { activityRuns, callGraph, graphWindow, observedGraph, nodeDescription } from "../../../lib/agent-flow/build";
  import { copyGeneratedText } from "../../../lib/generated-clipboard";
  import type { AgentFlowProps } from "../../../lib/agent-flow/model";

  let { query = "", dark, reduced, flowReport = null, activity = null, recording = null, flowLoading = false, activityLoading = false,
    flowError = null, activityError = null, demo = false, paused = false, onpausechange, onflowmodechange, onrefreshflow, onrefreshactivity,
    onstartrecording, onstoprecording, onclearactivity, onactivitysetup,
  }: AgentFlowProps & { query?: string; dark: boolean; reduced: boolean } = $props();

  let mode = $state<"connections" | "calls">("connections");
  let chosen = $state<string | null>(null);
  let runId = $state<string | null>(null);
  let setupOpen = $state(false);
  let busy = $state(false);
  let notice = $state("");
  let clearArmed = $state(false);
  let alive = true;
  onDestroy(() => { alive = false; });

  const runs = $derived(activityRuns(activity?.spans ?? [], query));
  const run = $derived(runs.find((r) => r.id === runId) ?? null);
  const windowed = $derived(mode === "calls" ? graphWindow(callGraph(run), chosen) : { graph: observedGraph(flowReport, query), omitted: 0 });
  const graph = $derived(windowed.graph);
  const loading = $derived(mode === "connections" ? flowLoading : activityLoading);
  const error = $derived(mode === "connections" ? flowError : activityError);
  const limits = $derived(mode === "connections" ? flowReport?.limits ?? [] : activity?.limits ?? []);
  const calls = $derived((activity?.spans ?? []).filter((s) => s.kind === "tool").length);
  const selected = $derived(chosen && (graph.nodes.some((n) => n.id === chosen) || graph.edges.some((e) => e.id === chosen)) ? chosen : null);
  const runName = (r: typeof runs[number]) => `${r.spans.find((s) => s.kind === "agent")?.name ?? r.spans[0]?.name ?? r.label} · ${r.label.toLowerCase()} · ${r.calls} tool ${r.calls === 1 ? "call" : "calls"}`;
  $effect(() => { if (!runs.some((r) => r.id === runId)) runId = runs[0]?.id ?? null; });
  $effect(() => { const next = mode; untrack(() => onflowmodechange?.(next)); });

  function change(next: "connections" | "calls") { mode = next; chosen = null; notice = ""; clearArmed = false; }
  async function action(fn: (() => Promise<void>) | undefined, success: string) {
    if (!fn || busy) return;
    busy = true; notice = "";
    try { await fn(); if (alive) notice = success; }
    catch { if (alive) notice = "That action could not be completed. Try again."; }
    finally { if (alive) busy = false; }
  }
  async function copySetup(provider: "codex" | "claude") {
    if (!onactivitysetup || busy) return;
    busy = true; notice = "";
    try {
      // Configuration contains a local bearer token; keep it transient and only copy on request.
      await copyGeneratedText(() => onactivitysetup!(provider), () => alive);
      if (alive) notice = `${provider === "codex" ? "Codex" : "Claude Code"} recording configuration copied. Merge it into your existing settings and restart that agent.`;
    } catch { if (alive) notice = "Configuration could not be copied. Check clipboard permissions and try again."; }
    finally { if (alive) busy = false; }
  }
</script>

<section class="flow-view" class:reduced aria-label="Agent flow">
  <header class="toolbar">
    <SegmentedControl value={mode} options={[{ value: "connections", label: "Connections", icon: "link" }, { value: "calls", label: "Calls", icon: "activity" }]} label="Flow data source" onchange={change} />
    <div class="toolbar-actions">
      {#if paused}<span class="paused">View paused</span>{/if}
      {#if onpausechange}<button type="button" aria-label={paused ? "Resume flow updates" : "Pause flow updates"} onclick={() => onpausechange(!paused)}><Icon name={paused ? "play" : "stop"} size={12} />{paused ? "Resume view" : "Pause view"}</button>{/if}
      <button type="button" disabled={loading} aria-label={mode === "calls" ? "Refresh recorded activity" : "Refresh connections"} onclick={() => mode === "calls" ? onrefreshactivity?.() : onrefreshflow?.()}><Icon name="refresh" size={12} />Refresh</button>
    </div>
  </header>
  <div class="source">
    <div><strong>{demo ? "Example data" : mode === "connections" ? "Operating system snapshot" : "Integration reported activity"}</strong>
      <p>{mode === "connections" ? "See which processes own tools and listeners, and which endpoints have live sockets. Sockets do not reveal messages or data direction." : "See recorded tool calls and parent spans from configured agents. Session membership and event order do not imply communication."}</p>
    </div>
    <span class="totals">{mode === "connections" ? `${graph.nodes.length} participants · ${graph.edges.length} relationships` : `${calls} tool calls · ${activity?.spans.length ?? 0} spans`}</span>
  </div>
  {#if error}<div class="load-error" role="status">Updates unavailable. {graph.nodes.length ? "Showing the last available data." : "Refresh to try again."}</div>{/if}
  {#if mode === "calls"}
    <div class="recording-bar">
      <span class="record-state" class:active={recording?.active}><i aria-hidden="true"></i>{demo ? "Example recording" : recording?.active ? "Recording on this computer" : "Recording off"}</span>
      {#if recording?.active}
        <button type="button" disabled={busy || !onstoprecording} onclick={() => action(onstoprecording, "Recording stopped. Captured metadata remains available.")}>Stop recording</button>
      {:else}
        <button type="button" disabled={busy || !onstartrecording || demo} onclick={() => action(onstartrecording, "Recording started. Configure an integration to receive activity.")}>Start recording</button>
      {/if}
      <button type="button" aria-expanded={setupOpen} onclick={() => setupOpen = !setupOpen}><Icon name="sliders" size={12} />Setup</button>
      {#if (activity?.spans.length ?? 0) > 0 && onclearactivity && !demo}
        <button type="button" class="clear" disabled={busy} onclick={() => clearArmed = !clearArmed}>Clear records</button>
      {/if}
      {#if (activity?.dropped ?? 0) > 0}<span class="dropped">{activity?.dropped} events dropped</span>{/if}
    </div>
    {#if clearArmed}<div class="clear-review"><span>Remove captured activity from memory? Recording stays as it is.</span><button type="button" disabled={busy} onclick={() => { clearArmed = false; void action(onclearactivity, "Captured activity cleared."); }}>Clear captured activity</button><button type="button" onclick={() => clearArmed = false}>Cancel</button></div>{/if}
    {#if setupOpen}
      <section class="setup" aria-label="Recording setup">
        <h3>Connect your agent</h3>
        <p>Start recording, copy settings for one provider, merge them with your existing configuration, then restart the agent. Only future reported activity appears. Choose one integration per provider to avoid duplicate counts.</p>
        {#if demo}<p>This preview uses example metadata. Recording is available in the local desktop app.</p>
        {:else}<div class="setup-actions"><button type="button" disabled={busy || !recording?.active || !onactivitysetup} onclick={() => copySetup("codex")}><Icon name="copy" size={12} />Copy Codex settings</button><button type="button" disabled={busy || !recording?.active || !onactivitysetup} onclick={() => copySetup("claude")}><Icon name="copy" size={12} />Copy Claude Code settings</button></div>{/if}
        <p class="setup-note">Configuration includes a private local token. Settings are never changed automatically. Claude Code tracing is a beta capability. Completed telemetry spans appear when exported; hooks may report a request without a completion.</p>
      </section>
    {/if}
  {/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if mode === "calls" && runs.length}
    <div class="run-picker"><label for="agent-flow-run">{run?.kind === "session" ? "Session" : "Run"}</label><select id="agent-flow-run" value={runId ?? ""} onchange={(e) => { runId = e.currentTarget.value; chosen = null; }}>{#each runs as r (r.id)}<option value={r.id}>{runName(r)}</option>{/each}</select>
      {#if run?.kind === "session"}<span>Session grouping · no parent correlation supplied</span>{/if}
      {#if run?.errors}<span class="failed-count">{run.errors} failed {run.errors === 1 ? "span" : "spans"}</span>{/if}
    </div>
  {/if}
  {#if windowed.omitted}<p class="window-note">Showing {graph.nodes.length} of {run?.spans.length} spans in the graph. All records remain in the timeline; select one to show it and its reported parents.</p>{/if}
  {#if graph.nodes.length}
    <div class="workspace">
      <FlowCanvas {graph} calls={mode === "calls"} chosen={selected} {dark} {reduced} onselect={(id) => chosen = id} />
      <aside class="companion" aria-label={mode === "calls" ? "Recorded activity details" : "Connection details"}>
        <FlowInspector {graph} chosen={selected} onclose={() => chosen = null} />
        {#if mode === "calls" && run}<ActivityTimeline {run} chosen={selected} onselect={(id) => chosen = id} />
        {:else}
          <section class="participants" aria-label="Flow participants"><h3>Participants <span>{graph.nodes.length}</span></h3><ul>{#each graph.nodes as n (n.id)}<li><button type="button" class:chosen={selected === n.id} aria-label={nodeDescription(n)} onclick={() => chosen = n.id}><strong>{n.label}</strong><span>{n.sub}</span><small>{n.kind} · {n.evidence}</small></button></li>{/each}</ul></section>
          <section class="relationships" aria-label="Flow relationships"><h3>Relationships <span>{graph.edges.length}</span></h3><ul>{#each graph.edges as e (e.id)}<li><button type="button" class:chosen={selected === e.id} onclick={() => chosen = e.id}><strong>{graph.nodes.find((n) => n.id === e.from)?.label} {e.kind === "tcp" ? "↔" : "→"} {graph.nodes.find((n) => n.id === e.to)?.label}</strong><span>{e.label} · {e.evidence}</span></button></li>{/each}</ul></section>
        {/if}
      </aside>
    </div>
  {:else}
    <div class="empty" aria-busy={loading}>
      <Icon name={mode === "calls" ? "activity" : "link"} size={28} />
      <h3>{loading ? "Loading flow…" : query.trim() ? "No flow matches this search" : mode === "calls" ? "No recorded activity yet" : "No observed connections yet"}</h3>
      <p>{query.trim() ? "Search by agent or tool name, process ID, address or port. Clear your search to see all available data." : mode === "calls" ? "Start recording and connect an integration to see reported calls. Running agents alone do not provide call telemetry." : "Start an agent or tool. Its observed process tree, listeners and live socket relationships appear after a scan."}</p>
      {#if mode === "calls" && !setupOpen}<button type="button" onclick={() => setupOpen = true}>Set up recording</button>{/if}
    </div>
  {/if}
  {#if limits.length || (flowReport && mode === "connections" && (flowReport.omitted_nodes || flowReport.omitted_edges))}
    <details class="limits" open={!graph.nodes.length}><summary><Icon name="info" size={12} />Visibility limits{#if mode === "connections" && flowReport && (flowReport.omitted_nodes || flowReport.omitted_edges)} · {flowReport.omitted_nodes} participants and {flowReport.omitted_edges} relationships omitted{/if}</summary><ul>{#each limits as limit}<li>{limit}</li>{/each}</ul></details>
  {/if}
</section>

<style>
  .flow-view { min-height: 0; height: 100%; display: flex; flex-direction: column; color: var(--text); container-type: inline-size; background: var(--bg); }
  .toolbar, .recording-bar, .run-picker { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .toolbar { justify-content: space-between; }
  .toolbar-actions { display: flex; align-items: center; gap: 6px; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 4px 9px; background: var(--surface); border: 1px solid var(--border); border-radius: 7px; color: var(--text-2); cursor: pointer; font: inherit; font-size: var(--fs-caption); }
  button:hover:not(:disabled) { border-color: var(--border-strong); background: var(--surface-2); } button:disabled { opacity: .5; cursor: not-allowed; } button:focus-visible, select:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .source { display: flex; gap: 14px; align-items: center; justify-content: space-between; padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .source strong { font-size: var(--fs-body-sm); } .source p { margin: 4px 0 0; max-width: 850px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-body); }
  .totals, .paused, .dropped { font-size: var(--fs-caption); color: var(--muted); white-space: nowrap; } .dropped { color: var(--warn); }
  .record-state { display: inline-flex; align-items: center; gap: 6px; margin-right: 3px; font-size: var(--fs-caption); color: var(--muted); }
  .record-state i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); } .record-state.active i { background: var(--tone-green); } .record-state.active { color: var(--text-2); }
  .clear { margin-left: auto; }
  .setup, .clear-review { padding: 12px 14px; background: var(--surface-2); border-bottom: 1px solid var(--border); }
  .setup h3 { font-size: var(--fs-body-sm); margin: 0; } .setup p { max-width: 900px; margin: 5px 0 10px; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-body); }
  .setup p.setup-note { margin-bottom: 0; color: var(--muted); } .setup-actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .clear-review { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: var(--fs-caption); } .clear-review span { flex: 1; }
  .load-error, .notice { margin: 0; padding: 8px 14px; background: var(--surface-2); color: var(--warn); border-bottom: 1px solid var(--border); font-size: var(--fs-caption); }
  .notice { color: var(--text-2); }
  .window-note { margin: 0; padding: 8px 14px; color: var(--muted); background: var(--surface); border-bottom: 1px solid var(--border); font-size: var(--fs-caption); }
  .run-picker label { font-size: var(--fs-caption); font-weight: var(--fw-semibold); } .run-picker select { max-width: min(650px, 80%); min-width: 0; padding: 5px 8px; border: 1px solid var(--border); border-radius: 6px; color: var(--text); background: var(--surface); font: inherit; font-size: var(--fs-caption); } .run-picker span { color: var(--muted); font-size: var(--fs-caption); }
  .workspace { flex: 1; min-height: 180px; display: grid; grid-template-columns: minmax(0, 1fr) 330px; }
  .failed-count { color: var(--danger) !important; }
  .companion { overflow: auto; min-height: 0; border-left: 1px solid var(--border); background: var(--surface); }
  .participants h3, .relationships h3 { margin: 0; padding: 12px 14px 6px; font-size: var(--fs-body-sm); } h3 span { color: var(--muted); font-weight: var(--fw-regular); }
  .participants ul, .relationships ul { list-style: none; margin: 0; padding: 0 8px 10px; }
  .participants button, .relationships button { display: grid; width: 100%; gap: 3px; padding: 9px 6px; border-color: transparent; border-bottom-color: var(--border); border-radius: 7px; background: transparent; text-align: left; justify-content: stretch; }
  .participants strong, .relationships strong { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); overflow-wrap: anywhere; } .participants span, .relationships span, .participants small { color: var(--muted); font-size: var(--fs-caption); overflow-wrap: anywhere; }
  .participants button.chosen, .relationships button.chosen { border-color: var(--accent); background: var(--accent-soft); }
  .empty { flex: 1; min-height: 120px; padding: 25px 18px; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 9px; text-align: center; color: var(--muted); }
  .empty h3 { margin: 0; color: var(--text); font-size: var(--fs-heading); } .empty p { max-width: 440px; margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body); }
  .limits { border-top: 1px solid var(--border); padding: 8px 14px; font-size: var(--fs-caption); color: var(--muted); } .limits summary { display: flex; gap: 6px; align-items: center; color: var(--text-2); cursor: pointer; } .limits ul { margin: 8px 0; padding-left: 18px; max-height: 110px; overflow: auto; }
  @container (max-width: 940px) { .workspace { grid-template-columns: minmax(0, 1fr) 285px; } .totals { display: none; } }
  @container (max-width: 940px) {
    .workspace { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(180px, 42%) minmax(0, 1fr); overflow: hidden; }
    .companion { overflow: auto; border-left: 0; border-top: 1px solid var(--border); }
    .companion :global(.inspector) { max-height: calc(100% - 24px); box-sizing: border-box; overflow: auto; }
    .source { align-items: flex-start; }
  }
  .reduced :global(*) { animation: none !important; transition: none !important; }
</style>
