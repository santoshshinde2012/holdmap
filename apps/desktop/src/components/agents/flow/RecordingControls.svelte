<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "../../Icon.svelte";
  import { copyGeneratedText } from "../../../lib/generated-clipboard";
  import type { RecordingStatus } from "../../../lib/agent-flow/model";

  let { recording = null, statusLoading = false, statusError = false, records = 0, dropped = 0, demo = false,
    onstartrecording, onstoprecording, onclearactivity, onactivitysetup,
  }: {
    recording?: RecordingStatus | null;
    statusLoading?: boolean;
    statusError?: boolean;
    records?: number;
    dropped?: number;
    demo?: boolean;
    onstartrecording?: () => Promise<void>;
    onstoprecording?: () => Promise<void>;
    onclearactivity?: () => Promise<void>;
    onactivitysetup?: (provider: "codex" | "claude") => Promise<string>;
  } = $props();

  let setupOpen = $state(false);
  let busy = $state(false);
  let notice = $state("");
  let clearArmed = $state(false);
  let alive = true;
  onDestroy(() => { alive = false; });

  // The empty Flow state can open this panel without owning its operational state.
  export function openSetup() { setupOpen = true; }
  export function setupVisible() { return setupOpen; }

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

    <div class="recording-bar">
      <span class="record-state" class:active={!demo && recording?.active}><i aria-hidden="true"></i>{demo ? "Example recording" : recording == null ? statusLoading ? "Checking recording status" : "Recording status unavailable" : recording.active ? "Recording on this computer" : "Recording off"}</span>
      {#if recording?.active && !demo}
        <button type="button" disabled={busy || !onstoprecording} onclick={() => action(onstoprecording, "Recording stopped. Captured metadata remains available.")}>Stop recording</button>
      {:else}
        <button type="button" disabled={busy || !recording || !onstartrecording || demo} onclick={() => action(onstartrecording, "Recording started. Configure an integration to receive activity.")}>Start recording</button>
      {/if}
      <button type="button" aria-expanded={setupOpen} onclick={() => setupOpen = !setupOpen}><Icon name="sliders" size={12} />Setup</button>
      {#if records > 0 && onclearactivity && !demo}
        <button type="button" class="clear" disabled={busy} onclick={() => clearArmed = !clearArmed}>Clear records</button>
      {/if}
      {#if dropped > 0}<span class="dropped">{dropped} events dropped</span>{/if}
      {#if statusError && recording && !demo}<span class="stale">Last known recording status</span>{/if}
    </div>
    {#if !demo && !recording && !statusLoading}<p class="status-unavailable" role="status">Refresh to check recording status before starting or stopping recording.</p>{/if}
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
{#if notice}<p class="notice" role="status">{notice}</p>{/if}

<style>
  .recording-bar { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 4px 9px; background: var(--surface); border: 1px solid var(--border); border-radius: 7px; color: var(--text-2); cursor: pointer; font: inherit; font-size: var(--fs-caption); }
  button:hover:not(:disabled) { border-color: var(--border-strong); background: var(--surface-2); } button:disabled { opacity: .5; cursor: not-allowed; } button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .record-state { display: inline-flex; align-items: center; gap: 6px; margin-right: 3px; font-size: var(--fs-caption); color: var(--muted); }
  .record-state i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); } .record-state.active i { background: var(--tone-green); } .record-state.active { color: var(--text-2); }
  .clear { margin-left: auto; }
  .setup, .clear-review { padding: 12px 14px; background: var(--surface-2); border-bottom: 1px solid var(--border); }
  .setup h3 { font-size: var(--fs-body-sm); margin: 0; } .setup p { max-width: 900px; margin: 5px 0 10px; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-body); }
  .setup p.setup-note { margin-bottom: 0; color: var(--muted); } .setup-actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .clear-review { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: var(--fs-caption); } .clear-review span { flex: 1; }
  .notice { color: var(--text-2); }
  .dropped, .stale { font-size: var(--fs-caption); color: var(--muted); white-space: nowrap; } .dropped { color: var(--warn); }
  .notice, .status-unavailable { margin: 0; padding: 8px 14px; background: var(--surface-2); color: var(--text-2); border-bottom: 1px solid var(--border); font-size: var(--fs-caption); }
  .status-unavailable { color: var(--muted); }
</style>
