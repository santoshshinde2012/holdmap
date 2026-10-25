<script lang="ts">
  // Bottom status bar: port summary, keyboard hints and the local capture tools (record a
  // screen, take a snapshot), kept here so the title bar stays for everyday actions.
  import { IconButton, Kbd } from "../ui";
  import type { PortStats } from "../../lib/port-list";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { runFraction } from "../../lib/security-view";

  let {
    stats,
    hints,
    mod,
    singleKeys,
    shortcut,
    recording,
    recordingResult,
    snapshot,
    onrecord,
    onsnapshot,
  }: {
    stats: PortStats | null;
    /** "flow" shows the flow-canvas hint; "agents" and "home" hide the stop hint. */
    hints: "list" | "agents" | "flow" | "home";
    mod: string;
    singleKeys: boolean;
    shortcut: string | null;
    recording: boolean;
    recordingResult: boolean;
    snapshot: { open: boolean; busy: boolean; hasResult: boolean };
    onrecord: () => void;
    onsnapshot: () => void;
  } = $props();
</script>

<footer class="status" aria-label="Summary">
  {#if stats}
    <span><b>{stats.total}</b> ports</span><span class="g"><b>{stats.dev}</b> dev</span>{#if stats.exposed}<span class="w"><b>{stats.exposed}</b> exposed</span>{/if}
    <span class="sp"></span>
    <span class="hints">{#if hints === "flow"}<span><Kbd keys="Tab" size="sm" />flow controls</span>{:else if hints === "home" && singleKeys}<span><Kbd keys="V" size="sm" />cards ↔ map</span>{:else}<span><Kbd keys={["↑", "↓"]} size="sm" />move</span>{#if hints === "list"}<span><Kbd keys="⌫" size="sm" />stop</span>{/if}{/if}{#if singleKeys}<span class="opt"><Kbd keys="G" size="sm" />graph</span><span class="opt"><Kbd keys={["⇧", "A"]} size="sm" />agents</span>{/if}<span><Kbd keys={[mod, "K"]} size="sm" />commands</span>{#if singleKeys}<span><Kbd keys="?" size="sm" />shortcuts</span>{/if}{#if shortcut}<span class="gs"><Kbd keys={shortcut} size="sm" />from anywhere</span>{/if}</span>
  {:else}
    <span class="sp"></span>
  {/if}
  {#if securityRuns.visible}
    <button type="button" class="run ph-{securityRuns.progress.phase}" data-status-run aria-label={securityRuns.busy ? `Analyzer run in progress, ${Math.round(runFraction(securityRuns.progress) * 100)}%. Show details` : `Analyzer run finished: ${securityRuns.summary}. Show details`} onclick={() => (securityRuns.collapsed = false)}>
      {#if securityRuns.busy}<span class="spin" aria-hidden="true"></span>{:else}<span class="dot" aria-hidden="true"></span>{/if}
      {securityRuns.busy ? `Scanning ${Math.min(securityRuns.progress.done + 1, securityRuns.progress.total)}/${securityRuns.progress.total}` : securityRuns.progress.phase === "done" ? "Scan done" : securityRuns.progress.phase === "cancelled" ? "Scan cancelled" : "Scan failed"}
    </button>
  {/if}
  <div class="capture" role="group" aria-label="Capture">
    <IconButton size="sm" placement="top" icon="video" label={recording ? "Recording screen" : recordingResult ? "Review recording" : "Record screen"} data-screen-record kbd={singleKeys ? ["⇧", "V"] : undefined} pressed={recording || recordingResult} onclick={onrecord} />
    <IconButton size="sm" placement="top" icon={snapshot.busy ? "x" : "camera"} label={snapshot.busy ? "Cancel snapshot capture" : snapshot.hasResult ? "Review snapshot" : "Take snapshot"} data-screen-snapshot kbd={singleKeys && !snapshot.busy ? ["⇧", "S"] : undefined} pressed={snapshot.open || snapshot.busy || snapshot.hasResult} onclick={onsnapshot} />
  </div>
</footer>

<style>
  .status { display: flex; align-items: center; gap: var(--sp-4); height: 32px; padding: 0 var(--sp-2) 0 var(--sp-4); border-top: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; }
  .status b { color: var(--text); font-weight: var(--fw-medium); }
  .g b { color: var(--tone-green); }
  .w b { color: var(--warn); }
  .sp { flex: 1; }
  .hints { display: inline-flex; align-items: center; gap: 14px; min-width: 0; overflow: hidden; }
  .hints > span { display: inline-flex; align-items: center; gap: 6px; }
  .run { display: inline-flex; align-items: center; gap: 6px; flex: none; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font: inherit; font-size: var(--fs-caption); cursor: pointer; }
  .run:hover { color: var(--text); border-color: var(--border-strong); }
  .run:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .run .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .run.ph-failed .dot { background: var(--danger); } .run.ph-cancelled .dot { background: var(--border-strong); }
  .spin { width: 10px; height: 10px; border: 2px solid var(--accent-soft); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  .capture { display: flex; align-items: center; gap: 1px; flex: none; padding-left: var(--sp-2); border-left: 1px solid var(--border); }
  /* Trim the hints in priority order instead of clipping the last one mid-word. */
  @media (max-width: 1100px) { .hints .gs { display: none; } }
  @media (max-width: 900px) { .hints .opt { display: none; } }
  @media (max-width: 760px) { .hints { display: none; } }
</style>
