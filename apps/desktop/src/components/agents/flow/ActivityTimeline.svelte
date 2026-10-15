<script lang="ts">
  import type { ActivityRun } from "../../../lib/agent-flow/build";
  import { activityName, activityStatus, duration, isIncomplete, missingParent, spanNodeId } from "../../../lib/agent-flow/build";
  import { activityRows } from "../../../lib/agent-flow/triage";
  import { PROVIDER_LABEL, type ActivitySpan } from "../../../lib/agent-flow/model";
  let { run, rows, chosen, onselect }: { run: ActivityRun; rows?: ActivitySpan[]; chosen: string | null; onselect: (id: string) => void } = $props();
  const spans = $derived(rows ?? activityRows(run));
  const time = (n: number | null) => n == null ? "Time unavailable" : new Date(n).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
</script>

<section class="timeline" aria-label="Recorded activity timeline">
  <header><h3>Activity timeline <span>{spans.length} of {run.spans.length}</span></h3><p>Event order is not proof of communication.</p></header>
  {#if !spans.length}<p class="empty">No activity matches this view. Reset the filters or clear your search to see the recorded run.</p>{/if}
  <ol>
    {#each spans as span (span.id)}
      <li><button type="button" class:chosen={chosen === spanNodeId(span)} aria-pressed={chosen === spanNodeId(span)} onclick={() => onselect(spanNodeId(span))}>
        <span class="time">{time(span.start_at_ms ?? span.end_at_ms)}{#if span.start_at_ms != null || span.end_at_ms != null}<small>{span.kind === "skill" ? "Activated" : span.start_at_ms != null ? "Started" : "Ended"}</small>{/if}</span>
        <span class="body"><strong>{activityName(span)}</strong><span>{span.kind === "skill" ? "Skill activation" : span.kind} · {PROVIDER_LABEL[span.provider]}</span>
          {#if span.mcp_server}<small>MCP server: {span.mcp_server} · reported alias</small>{/if}
          {#if missingParent(span, run)}<small>Parent span not received</small>{/if}
          {#if isIncomplete(span)}<small>No completion received</small>{/if}
        </span>
        <span class="status" class:error={span.status === "error"}>{activityStatus(span)}<small>{duration(span)}</small></span>
      </button></li>
    {/each}
  </ol>
</section>

<style>
  .timeline { min-height: 0; }
  header { padding: 12px 14px 8px; border-top: 1px solid var(--border); }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  h3 span { color: var(--muted); font-weight: var(--fw-regular); }
  p { margin: 4px 0 0; font-size: var(--fs-caption); color: var(--muted); }
  .empty { padding: 4px 14px 12px; line-height: var(--lh-body); }
  ol { list-style: none; padding: 0 8px 12px; margin: 0; }
  button { display: grid; grid-template-columns: 65px minmax(0, 1fr); gap: 5px 8px; width: 100%; text-align: left; padding: 10px 6px; border: 1px solid transparent; border-bottom-color: var(--border); background: transparent; border-radius: 7px; color: var(--text); font: inherit; cursor: pointer; }
  button:hover { background: var(--surface-2); } button.chosen { border-color: var(--accent); background: var(--accent-soft); } button:focus-visible { outline: 2px solid var(--ring); outline-offset: -1px; }
  .time { font-family: var(--mono); font-size: var(--fs-mono-sm); color: var(--muted); padding-top: 2px; font-variant-numeric: tabular-nums; } .time small { display: block; margin-top: 4px; font-family: var(--font); }
  .body { display: grid; gap: 3px; min-width: 0; } strong { font-size: var(--fs-body-sm); overflow-wrap: anywhere; }
  .body span, small { font-size: var(--fs-caption); color: var(--muted); } .body small { color: var(--warn); }
  .status { grid-column: 2; display: flex; flex-wrap: wrap; gap: 4px 8px; font-size: var(--fs-caption); color: var(--text-2); } .status.error { color: var(--danger); }
</style>
