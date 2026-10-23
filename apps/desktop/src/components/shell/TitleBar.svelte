<script lang="ts">
  // App title bar: brand, the view's search field, live status and the everyday tools. Capture
  // tools live in the status bar; only an active recording shows here, because it's live state.
  import type { Snippet } from "svelte";
  import { IconButton } from "../ui";
  import { formatClock } from "../../lib/screen-record";
  import { tooltip } from "../../lib/tooltip";

  let {
    searchbox,
    liveText,
    liveStale,
    livePulse,
    modK,
    mod,
    singleKeys,
    refreshing,
    manualRefresh,
    recordingMs = null,
    recordingLimitMs = null,
    recordingPaused = false,
    canPauseRecording = false,
    scheduleCount,
    themeIcon,
    themeLabel,
    schedulesTrigger = $bindable(),
    onpalette,
    onrefresh,
    onstoprecording,
    onpauserecording = () => {},
    onhistory,
    onschedules,
    onremote,
    ontheme,
    onsettings,
  }: {
    searchbox: Snippet;
    liveText: string;
    liveStale: boolean;
    livePulse: boolean;
    modK: string;
    mod: string;
    singleKeys: boolean;
    refreshing: boolean;
    manualRefresh: boolean;
    /** Elapsed time of an active screen recording; null when not recording. */
    recordingMs?: number | null;
    /** Length at which the recording stops by itself; shows the time left near the end. */
    recordingLimitMs?: number | null;
    recordingPaused?: boolean;
    canPauseRecording?: boolean;
    scheduleCount: number;
    themeIcon: string;
    themeLabel: string;
    schedulesTrigger?: HTMLButtonElement;
    onpalette: () => void;
    onrefresh: () => void;
    onstoprecording: () => void;
    onpauserecording?: () => void;
    onhistory: () => void;
    onschedules: () => void;
    onremote: () => void;
    ontheme: () => void;
    onsettings: () => void;
  } = $props();
  /** The last 30 seconds (or fifth, for short limits) before an automatic stop. */
  const endingSoon = $derived(recordingMs !== null && !!recordingLimitMs && !recordingPaused && recordingLimitMs - recordingMs <= Math.min(30_000, recordingLimitMs / 5));
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={`${import.meta.env.BASE_URL}icon.svg`} alt="" width="22" height="22" />
    <h1 class="name">holdmap</h1>
  </div>

  <div class="search">{@render searchbox()}</div>

  <div class="tools">
    <span class="live" class:stale={liveStale} aria-live="off" use:tooltip={liveText}>
      <span class="pulse" class:on={livePulse}></span><span class="live-text">{liveText}</span>
    </span>
    <div class="cluster">
      <IconButton icon="command" label="Commands" kbd={modK} onclick={onpalette} aria-keyshortcuts="Meta+K Control+K" />
      <IconButton icon="refresh" label="Refresh" kbd={singleKeys ? "R" : undefined} onclick={onrefresh} disabled={refreshing} aria-busy={manualRefresh || undefined} class={manualRefresh ? "spinning" : ""} />
    </div>
    {#if recordingMs !== null}
      <div class="cluster rec" class:paused={recordingPaused} class:ending={endingSoon}>
        <span class="rec-dot" aria-hidden="true"></span>
        <span class="rec-time" title={recordingLimitMs ? `Stops by itself after ${formatClock(recordingLimitMs)}` : undefined}>{formatClock(recordingMs)}{#if endingSoon && recordingLimitMs} <span class="rec-left">· {formatClock(recordingLimitMs - recordingMs)} left</span>{/if}</span>
        <span class="sr-only" role="status">{recordingPaused ? "Recording paused" : endingSoon ? "Recording stops soon" : "Recording"}</span>
        {#if canPauseRecording}<IconButton icon={recordingPaused ? "play" : "pause"} label={recordingPaused ? "Resume screen recording" : "Pause screen recording"} aria-pressed={recordingPaused} onclick={onpauserecording} />{/if}
        <IconButton icon="stop" label="Stop screen recording" data-screen-record kbd={singleKeys ? ["⇧", "V"] : undefined} onclick={onstoprecording} />
      </div>
    {/if}
    <div class="cluster">
      <IconButton icon="history" label="Recently stopped" kbd={singleKeys ? "H" : undefined} onclick={onhistory} />
      <IconButton icon="clock" label={scheduleCount ? `Scheduled actions (${scheduleCount} active)` : "Scheduled actions"} badge={scheduleCount || undefined} onclick={onschedules} bind:el={schedulesTrigger} />
      <IconButton icon="server" label="Remote host…" onclick={onremote} />
    </div>
    <div class="cluster">
      <IconButton icon={themeIcon} label="Theme: {themeLabel}" kbd={singleKeys ? ["⇧", "L"] : undefined} onclick={ontheme} />
      <IconButton icon="sliders" label="Settings" kbd={[mod, ","]} onclick={onsettings} />
    </div>
  </div>
</header>

<style>
  .titlebar { display: flex; align-items: center; gap: var(--sp-4); height: 52px; padding: 0 var(--sp-3) 0 var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); }
  :global(.app.mac) .titlebar { padding-left: 84px; }
  .brand { display: flex; align-items: center; gap: var(--sp-2); flex: none; }
  .brand img { border-radius: 6px; box-shadow: var(--shadow-sm); }
  .name { margin: 0; font-weight: var(--fw-semibold); font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); }
  .search { flex: 1; max-width: 520px; margin: 0 auto; min-width: 140px; }
  .tools { display: flex; align-items: center; gap: 8px; flex: none; }
  .cluster { display: flex; align-items: center; gap: 1px; padding: 2px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .cluster.rec { gap: 6px; padding: 2px 4px 2px 10px; color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .rec-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--danger); box-shadow: 0 0 0 3px var(--danger-soft); }
  .cluster.rec.paused .rec-dot { background: var(--muted); box-shadow: 0 0 0 3px var(--surface-2); animation: none; }
  .cluster.rec.ending .rec-left { font-weight: var(--fw-regular); }
  .rec-time { font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; font-weight: var(--fw-semibold); min-width: 2.4em; }
  @media (prefers-reduced-motion: no-preference) { .rec-dot { animation: beat 1.1s ease-in-out infinite; } }
  .tools :global(.spinning svg) { animation: spin 0.9s linear infinite; }
  .live { position: relative; display: inline-flex; align-items: center; gap: 7px; height: var(--h-md); padding: 0 10px 0 8px; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--border); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .pulse.on { animation: beat 0.9s ease-in-out infinite; }
  .live.stale .pulse { background: var(--warn); box-shadow: 0 0 0 3px var(--warn-soft); }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.6; } }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 760px) { .brand .name, .live-text { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; } .live { padding: 0; width: var(--h-md); justify-content: center; border: 0; background: transparent; } .titlebar { gap: var(--sp-2); } }
  @media (max-width: 560px) {
    .titlebar { display: grid; grid-template-columns: auto minmax(0, 1fr); height: auto; padding-top: var(--sp-2); padding-bottom: var(--sp-2); }
    :global(.app.mac) .titlebar { padding-left: var(--sp-4); }
    .search { width: 100%; min-width: 0; }
    .tools { grid-column: 1 / -1; justify-self: end; flex-wrap: wrap; justify-content: flex-end; max-width: 100%; }
  }
</style>
