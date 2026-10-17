<script lang="ts">
  // Bottom status bar: port summary, keyboard hints and the local capture tools (record a
  // screen, take a snapshot), kept here so the title bar stays for everyday actions.
  import { IconButton, Kbd } from "../ui";
  import type { PortStats } from "../../lib/port-list";

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
    <span class="hints">{#if hints === "flow"}<span><Kbd keys="Tab" size="sm" />flow controls</span>{:else}<span><Kbd keys={["↑", "↓"]} size="sm" />move</span>{#if hints === "list"}<span><Kbd keys="⌫" size="sm" />stop</span>{/if}{/if}{#if singleKeys}<span><Kbd keys="G" size="sm" />graph</span><span><Kbd keys={["⇧", "A"]} size="sm" />agents</span>{/if}<span><Kbd keys={[mod, "K"]} size="sm" />commands</span>{#if singleKeys}<span><Kbd keys="?" size="sm" />shortcuts</span>{/if}{#if shortcut}<span class="gs"><Kbd keys={shortcut} size="sm" />from anywhere</span>{/if}</span>
  {:else}
    <span class="sp"></span>
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
  .capture { display: flex; align-items: center; gap: 1px; flex: none; padding-left: var(--sp-2); border-left: 1px solid var(--border); }
  @media (max-width: 760px) { .hints { display: none; } }
</style>
