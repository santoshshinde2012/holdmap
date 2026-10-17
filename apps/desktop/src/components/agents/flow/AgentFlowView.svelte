<script lang="ts">
  import { untrack } from "svelte";
  import { ActionDock, ActionMenu, ActionPill, EvidenceChip, type MenuItem } from "../../ui";
  import Icon from "../../Icon.svelte";
  import FlowCanvas from "./FlowCanvas.svelte";
  import FlowInspector from "./FlowInspector.svelte";
  import ActivityTimeline from "./ActivityTimeline.svelte";
  import RecordingControls from "./RecordingControls.svelte";
  import { activityMatches, activityRuns, graphWindow, observedGraph, nodeDescription } from "../../../lib/agent-flow/build";
  import { activityContext, activityRows, type ActivityFilter, type ActivityOrder } from "../../../lib/agent-flow/triage";
  import type { AgentFlowProps } from "../../../lib/agent-flow/model";
  import { prefGetBoolean, prefSet } from "../../../lib/prefs";

  let { query = "", dark, reduced, flowReport = null, activity = null, recording = null, flowLoading = false, activityLoading = false,
    flowError = null, activityError = null, demo = false, paused = false, onpausechange, onflowmodechange, onrefreshflow, onrefreshactivity,
    onstartrecording, onstoprecording, onclearactivity, onactivitysetup, onclearquery,
    mode = $bindable("connections"), sourceChrome = true,
  }: AgentFlowProps & { query?: string; dark: boolean; reduced: boolean; mode?: "connections" | "calls"; sourceChrome?: boolean } = $props();

  let inspector = $state(prefGetBoolean("flow.inspector", true));
  $effect(() => prefSet("flow.inspector", inspector ? "1" : "0"));
  let showLimits = $state(prefGetBoolean("flow.limits", true));
  $effect(() => prefSet("flow.limits", showLimits ? "1" : "0"));
  let showRecording = $state(prefGetBoolean("flow.recording", true));
  $effect(() => prefSet("flow.recording", showRecording ? "1" : "0"));
  let chosen = $state<string | null>(null);
  let runId = $state<string | null>(null);
  let filter = $state<ActivityFilter>("all");
  let order = $state<ActivityOrder>("time");
  let recordingControls = $state<{ openSetup(): void; setupVisible(): boolean }>();

  const allRuns = $derived(activityRuns(activity?.spans ?? []));
  // Search narrows the picker while retaining the current run and its inspected record.
  const runs = $derived(allRuns.filter((r) => r.id === runId || r.spans.some((s) => activityMatches(s, query))));
  const run = $derived(allRuns.find((r) => r.id === runId) ?? null);
  const rows = $derived(activityRows(run, query, filter, order));
  const context = $derived(activityContext(run, rows, chosen));
  const windowed = $derived(mode === "calls" ? graphWindow(context, chosen) : { graph: observedGraph(flowReport, query), omitted: 0 });
  const graph = $derived(windowed.graph);
  const loading = $derived(mode === "connections" ? flowLoading : activityLoading);
  const error = $derived(mode === "connections" ? flowError : activityError);
  const limits = $derived(mode === "connections" ? flowReport?.limits ?? [] : activity?.limits ?? []);
  const calls = $derived((activity?.spans ?? []).filter((s) => s.kind === "tool").length);
  const skills = $derived((activity?.spans ?? []).filter((s) => s.kind === "skill").length);
  const selected = $derived(chosen && (graph.nodes.some((n) => n.id === chosen) || graph.edges.some((e) => e.id === chosen)) ? chosen : null);
  const canInspect = $derived(graph.nodes.length > 0 || (mode === "calls" && !!run));
  const canLimit = $derived(limits.length > 0 || (mode === "connections" && !!flowReport && (flowReport.omitted_nodes > 0 || flowReport.omitted_edges > 0)));
  const sourceLabel = $derived(demo ? "Example data" : mode === "connections" ? "OS snapshot" : "Reported activity");
  const sourceHint = $derived(mode === "connections"
    ? "See which processes own tools and listeners, and which endpoints have live sockets. Sockets do not reveal messages or data direction."
    : "See reported tool calls, skill activations and parent spans. Session membership and event order do not imply communication or a match to configured capabilities.");
  // Secondary view toggles live in one overflow menu so the toolbar keeps to source, refresh and pause.
  const viewItems = $derived<MenuItem[]>([
    ...(canInspect ? [{ id: "inspector", icon: "panel", label: "Inspector", checked: inspector, run: () => (inspector = !inspector) }] : []),
    ...(canLimit ? [{ id: "limits", icon: "info", label: "Visibility limits", checked: showLimits, run: () => (showLimits = !showLimits) }] : []),
    ...(mode === "calls" ? [{ id: "recording", icon: "activity", label: "Recording controls", checked: showRecording, run: () => (showRecording = !showRecording) }] : []),
  ]);
  const sourceTotals = $derived(mode === "connections" ? `${graph.nodes.length} participants · ${graph.edges.length} relationships` : `${calls} tool calls${skills ? ` · ${skills} skill ${skills === 1 ? "activation" : "activations"}` : ""} · ${activity?.spans.length ?? 0} spans`);
  const runName = (r: typeof runs[number]) => `${r.spans.find((s) => s.kind === "agent")?.name ?? r.spans[0]?.name ?? r.label} · ${r.label.toLowerCase()} · ${r.calls} tool ${r.calls === 1 ? "call" : "calls"}${r.skills ? ` · ${r.skills} skill ${r.skills === 1 ? "activation" : "activations"}` : ""}`;
  $effect(() => { if (!allRuns.some((r) => r.id === runId)) runId = runs[0]?.id ?? null; });
  $effect.pre(() => {
    const next = mode;
    untrack(() => {
      chosen = null;
      onflowmodechange?.(next);
    });
  });

  function change(next: "connections" | "calls") { mode = next; }
  function resetView() { filter = "all"; order = "time"; onclearquery?.(); }
</script>

<section class="flow-view" class:reduced aria-label="Agent flow">
  <header class="toolbar">
    {#if sourceChrome}
      <ActionDock label="Flow data source" value={mode} options={[{ value: "connections", label: "Connections", icon: "link" }, { value: "calls", label: "Calls", icon: "activity" }]} onchange={change} />
    {/if}
    <div class="source">
      <EvidenceChip tone={demo ? "unknown" : mode === "connections" ? "seen" : "configured"} label={sourceLabel} detail={sourceHint} />
      <span class="totals">{sourceTotals}</span>
    </div>
    {#if paused}<span class="paused">View paused</span>{/if}
    {#if mode === "calls"}
      <RecordingControls bind:this={recordingControls} bar={showRecording} {recording} {demo} statusLoading={activityLoading} statusError={Boolean(activityError)}
        records={activity?.spans.length ?? 0} dropped={activity?.dropped ?? 0}
        {onstartrecording} {onstoprecording} {onclearactivity} {onactivitysetup} />
    {/if}
    <div class="toolbar-actions">
      {#if onpausechange}<ActionPill icon={paused ? "play" : "stop"} label={paused ? "Resume view" : "Pause view"} name={paused ? "Resume flow updates" : "Pause flow updates"} toggle pressed={paused} onclick={() => onpausechange(!paused)} />{/if}
      <ActionPill icon="refresh" label="Refresh" name={mode === "calls" ? "Refresh recorded activity" : "Refresh connections"} disabled={loading} onclick={() => mode === "calls" ? onrefreshactivity?.() : onrefreshflow?.()} />
      {#if viewItems.length}<ActionMenu label="View options" size="sm" placement="down" items={viewItems} />{/if}
    </div>
  </header>
  {#if error}<div class="load-error" role="status">Updates unavailable. {graph.nodes.length ? "Showing the last available data." : "Refresh to try again."}</div>{/if}
  {#if mode === "calls" && runs.length}
    <div class="run-picker"><label for="agent-flow-run">{run?.kind === "session" ? "Session" : "Run"}</label><select id="agent-flow-run" value={runId ?? ""} onchange={(e) => { runId = e.currentTarget.value; chosen = null; }}>{#each runs as r (r.id)}<option value={r.id}>{runName(r)}</option>{/each}</select>
      {#if run?.kind === "session"}<span>Session grouping · no parent correlation supplied</span>{/if}
      {#if run?.errors}<span class="failed-count">{run.errors} failed {run.errors === 1 ? "span" : "spans"}</span>{/if}
    </div>
  {/if}
  {#if mode === "calls" && run}
    <div class="activity-controls" role="group" aria-label="Recorded activity controls">
      <ActionDock hinge={false} animate={false} value={filter} options={[{ value: "all", label: "All" }, { value: "failed", label: "Failed" }, { value: "incomplete", label: "No completion" }]} label="Activity filter" onchange={(next) => filter = next} />
      <label for="agent-flow-order">Order</label><select id="agent-flow-order" bind:value={order}><option value="time">Chronological</option><option value="duration">Longest reported duration</option></select>
      <span class="shown-count">{rows.length} of {run.spans.length} records shown</span>
      <ActionPill label="Reset activity view" onclick={resetView} />
    </div>
  {/if}
  {#if windowed.omitted}<p class="window-note">Showing {graph.nodes.length} of {context.nodes.length} context spans in the graph. All matching records remain in the timeline; select one to show it and its reported parents.</p>{/if}
  {#if graph.nodes.length || (mode === "calls" && run)}
    <div class="workspace" class:full={!inspector}>
      {#if graph.nodes.length}<FlowCanvas {graph} calls={mode === "calls"} chosen={selected} {dark} {reduced} onselect={(id) => chosen = id} />
      {:else}<div class="empty"><Icon name="activity" size={28} /><h3>No activity matches this view</h3><p>Reset the filters or clear your search to see this recorded run.</p></div>{/if}
      {#if inspector}
      <aside class="companion" aria-label={mode === "calls" ? "Recorded activity details" : "Connection details"}>
        <FlowInspector {graph} chosen={selected} onclose={() => chosen = null} />
        {#if mode === "calls" && run}<ActivityTimeline {run} {rows} chosen={selected} onselect={(id) => chosen = id} />
        {:else}
          <section class="participants" aria-label="Flow participants"><h3>Participants <span>{graph.nodes.length}</span></h3><ul>{#each graph.nodes as n (n.id)}<li><button type="button" class:chosen={selected === n.id} aria-label={nodeDescription(n)} onclick={() => chosen = n.id}><strong>{n.label}</strong><span>{n.sub}</span><small>{n.kind} · {n.evidence}</small></button></li>{/each}</ul></section>
          <section class="relationships" aria-label="Flow relationships"><h3>Relationships <span>{graph.edges.length}</span></h3><ul>{#each graph.edges as e (e.id)}<li><button type="button" class:chosen={selected === e.id} onclick={() => chosen = e.id}><strong>{graph.nodes.find((n) => n.id === e.from)?.label} {e.kind === "tcp" ? "↔" : "→"} {graph.nodes.find((n) => n.id === e.to)?.label}</strong><span>{e.label} · {e.evidence}</span></button></li>{/each}</ul></section>
        {/if}
      </aside>
      {/if}
    </div>
  {:else}
    <div class="empty" aria-busy={loading}>
      <Icon name={mode === "calls" ? "activity" : "link"} size={28} />
      <h3>{loading ? "Loading flow…" : query.trim() ? "No flow matches this search" : mode === "calls" ? "No recorded activity yet" : "No observed connections yet"}</h3>
      <p>{query.trim() ? "Search by agent or tool name, process ID, address or port. Clear your search to see all available data." : mode === "calls" ? "Start recording and connect an integration to see reported calls. Running agents alone do not provide call telemetry." : "Start an agent or tool. Its observed process tree, listeners and live socket relationships appear after a scan."}</p>
      {#if query.trim() && onclearquery}<ActionPill label="Clear search" onclick={onclearquery} />
      {:else if mode === "calls" && !query.trim() && !recordingControls?.setupVisible()}<ActionPill label="Set up recording" onclick={() => { showRecording = true; recordingControls?.openSetup(); }} />{/if}
    </div>
  {/if}
  {#if showLimits && canLimit}
    <details class="limits"><summary><Icon name="info" size={12} />Visibility limits{#if mode === "connections" && flowReport && (flowReport.omitted_nodes || flowReport.omitted_edges)} · {flowReport.omitted_nodes} participants and {flowReport.omitted_edges} relationships omitted{/if}</summary><ul>{#each limits as limit}<li>{limit}</li>{/each}</ul></details>
  {/if}
</section>

<style>
  .flow-view { min-height: 0; height: 100%; display: flex; flex-direction: column; color: var(--text); container-type: inline-size; background: var(--bg); }
  .toolbar, .run-picker, .activity-controls { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 8px 14px; border-bottom: 1px solid var(--border); }
  .toolbar { gap: 10px; }
  .toolbar > :global(.dock) { order: 0; }
  .source { order: 1; display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1 1 160px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .paused { order: 2; }
  .toolbar > :global(.recording-bar) { order: 3; padding: 0; border: 0; flex: none; }
  .toolbar-actions { order: 4; display: flex; align-items: center; gap: 6px; margin-left: auto; }
  .toolbar > :global(.setup), .toolbar > :global(.clear-review), .toolbar > :global(.notice), .toolbar > :global(.status-unavailable) { order: 5; flex-basis: 100%; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 4px 9px; background: var(--surface); border: 1px solid var(--border); border-radius: 7px; color: var(--text-2); cursor: pointer; font: inherit; font-size: var(--fs-caption); }
  button:hover:not(:disabled) { border-color: var(--border-strong); background: var(--surface-2); } button:disabled { opacity: .5; cursor: not-allowed; } button:focus-visible, select:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .totals, .paused { font-size: var(--fs-caption); color: var(--muted); white-space: nowrap; }
  .load-error { margin: 0; padding: 8px 14px; background: var(--surface-2); color: var(--warn); border-bottom: 1px solid var(--border); font-size: var(--fs-caption); }
  .window-note { margin: 0; padding: 8px 14px; color: var(--muted); background: var(--surface); border-bottom: 1px solid var(--border); font-size: var(--fs-caption); }
  .run-picker label, .activity-controls label { font-size: var(--fs-caption); font-weight: var(--fw-semibold); } .run-picker select, .activity-controls select { max-width: min(650px, 80%); min-width: 0; padding: 5px 8px; border: 1px solid var(--border); border-radius: 6px; color: var(--text); background: var(--surface); font: inherit; font-size: var(--fs-caption); } .run-picker span, .shown-count { color: var(--muted); font-size: var(--fs-caption); }
  .workspace { flex: 1; min-height: 180px; display: grid; grid-template-columns: minmax(0, 1fr) 330px; }
  .workspace.full { grid-template-columns: minmax(0, 1fr); }
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
  @container (max-width: 940px) {
    .workspace:not(.full) { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(180px, 42%) minmax(0, 1fr); overflow: hidden; }
    .companion { overflow: auto; border-left: 0; border-top: 1px solid var(--border); }
    .companion :global(.inspector) { max-height: calc(100% - 24px); box-sizing: border-box; overflow: auto; }
    .totals { display: none; }
  }
  .reduced :global(*) { animation: none !important; transition: none !important; }
</style>
