<script lang="ts">
  import { untrack } from "svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Checkbox from "./ui/Checkbox.svelte";
  import Callout from "./ui/Callout.svelte";
  import ActionTiming from "./ActionTiming.svelte";
  import { resolveScheduleDelay, scheduleDeadline, type ScheduledAction } from "../lib/schedules";
  import { shutdownPreviewLifetimeMs, type ShutdownPreview } from "../lib/power";

  let { preview, loading, error, busy, requested, onconfirm, onclose, onretry, demo = false, onschedule, scheduled = null, onviewschedules, intent = "combined" }: {
    preview: ShutdownPreview | null;
    loading: boolean;
    error: string | null;
    busy: boolean;
    requested: boolean;
    onconfirm: () => void;
    onclose: () => void;
    onretry?: () => void;
    demo?: boolean;
    onschedule?: (delaySecs: number) => void;
    scheduled?: ScheduledAction | null;
    onviewschedules?: () => void;
    /** Separate entries keep immediate shutdown and scheduling unambiguous. */
    intent?: "combined" | "now" | "schedule";
  } = $props();

  let understood = $state(false);
  let submitted = $state(false);
  let expired = $state(true);
  let delaySecs = $state(untrack(() => intent === "schedule" ? 3600 : 0));
  let atMs = $state<number | null>(null);
  let timingError = $state<string | null>(null);
  let timingValid = $state(true);
  $effect(() => { void delaySecs; void atMs; understood = false; timingError = null; });
  const previewId = $derived(preview?.confirmation_id ?? null);
  $effect(() => {
    // A re-render of the same token must not renew its lifetime or acknowledgement.
    const id = previewId;
    const lifetime = id ? untrack(() => shutdownPreviewLifetimeMs(preview)) : 0;
    understood = false;
    submitted = false;
    expired = lifetime === 0;
    if (!lifetime) return;
    const timer = setTimeout(() => (expired = true), lifetime);
    return () => clearTimeout(timer);
  });
  const finished = $derived(requested || !!scheduled);
  const scheduling = $derived(intent === "schedule" || (intent !== "now" && !!onschedule && (atMs !== null || delaySecs !== 0)));
  const ready = $derived(!!preview && !expired && understood && timingValid && !timingError && !loading && !busy && !finished && !submitted && !error && (!scheduling || !!onschedule));
  const heading = $derived(intent === "schedule" ? demo ? "Schedule shutdown simulation?" : "Schedule computer shutdown?" : demo ? "Simulate computer shutdown?" : "Shut down this computer?");
  $effect(() => { if (expired || error) understood = false; });
  function confirm() {
    if (!ready) return;
    try {
      if (scheduling) {
        // Delay validation and an explicit callback are required; never fall through to now.
        const resolved = resolveScheduleDelay(delaySecs, atMs);
        if (!onschedule) return;
        submitted = true;
        onschedule(resolved);
      } else {
        submitted = true;
        onconfirm();
      }
    } catch (failure) { timingError = String(failure); }
  }
</script>

<Dialog role="alertdialog" title={heading} description="This affects the local computer, even when you're viewing a remote host." icon="monitor" iconTone="danger" size="md" dismissable={!busy} {onclose} initialFocus="[data-cancel]">
  <div class="content">
    {#if demo}
      <Callout tone="info" title="Browser demo">This is a simulation. No operating-system shutdown will be requested.</Callout>
    {/if}
    {#if scheduled}
      <div role="status"><Callout tone="ok" title={demo ? "Simulation scheduled" : "Shutdown scheduled"}>Scheduled for {scheduleDeadline(scheduled.due_at_ms)}. {demo ? "No operating-system shutdown will be requested." : "Keep Holdmap running and this computer awake. You can cancel before execution from Scheduled actions."}</Callout></div>
    {:else if requested}
      <div role="status"><Callout tone="ok" title={demo ? "Simulation complete" : "Shutdown requested"}>{demo ? "Your computer is still running. Nothing was stopped." : "Follow any operating-system prompts. Closing this dialog does not cancel the shutdown request. If the OS cancels afterward, quit Holdmap completely and reopen it before trying again."}</Callout></div>
    {:else}
      <Callout tone="warn" title="All apps, agents and local services will stop">Save your work and finish any important tasks before continuing. Unsaved work can be lost.</Callout>
      {#if loading}
        <p class="note" role="status">Checking this computer…</p>
      {:else if preview}
        <dl class="computer"><div><dt>Computer</dt><dd>{preview.hostname || "This computer"}</dd></div><div><dt>Operating system</dt><dd>{preview.platform}</dd></div></dl>
        {#if expired}
          <Callout tone="warn" title="Confirmation expired">Review a new preview before continuing. Nothing shuts down automatically.</Callout>
        {:else}
          <p class="note">Your operating system may ask for permission or cancel the request. This review expires after one minute.{intent !== "now" ? " A schedule is created only when you confirm it below." : ""}</p>
        {/if}
        {#if intent !== "now" && (onschedule || intent === "schedule")}
          <ActionTiming bind:delaySecs bind:atMs bind:valid={timingValid} scope="shutdown" {demo} allowNow={intent !== "schedule"} initialDelaySecs={intent === "schedule" ? 3600 : 0} disabled={expired || loading || busy || submitted || !!error || !onschedule} />
        {/if}
        {#if intent === "schedule" && !onschedule}<Callout tone="danger" title="Scheduling unavailable">Close this review and try again in a supported desktop session. Nothing has been requested.</Callout>{/if}
        <Checkbox tone="danger" bind:checked={understood} disabled={expired || loading || busy || submitted || !!error || !timingValid || (scheduling && !onschedule)} label={scheduling ? "I understand this computer will shut down at the scheduled time, and I will save my work before then." : "I saved my work and understand this shuts down all apps, agents and local services on this computer."} />
      {/if}
      {#if error}<Callout tone="danger" title="Could not confirm the shutdown request">{error}</Callout>{/if}
      {#if timingError}<Callout tone="danger" title="Choose a new scheduled time">{timingError}</Callout>{/if}
    {/if}
  </div>
  {#snippet footer()}
    <Button data-cancel variant="secondary" disabled={busy} onclick={onclose}>{finished ? "Done" : "Cancel"}</Button>
    {#if scheduled && onviewschedules}<Button variant="primary" icon="clock" onclick={onviewschedules}>View scheduled actions</Button>{/if}
    {#if !finished}
      {#if onretry && !loading && !busy && (expired || error || submitted)}
        <Button variant="secondary" onclick={onretry}>Review new preview</Button>
      {/if}
      <Button variant="danger" disabled={!ready} loading={busy} loadingText={scheduling ? "Scheduling…" : demo ? "Simulating…" : "Requesting shutdown…"} onclick={confirm}>{scheduling ? demo ? "Schedule simulation" : "Schedule shutdown" : demo ? "Simulate shutdown" : "Shut down this computer"}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .content { display: flex; flex-direction: column; gap: var(--sp-4); }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); }
  .computer { margin: 0; padding: var(--sp-3); display: grid; gap: var(--sp-2); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); font-size: var(--fs-body); line-height: var(--lh-body); }
  .computer div { display: grid; grid-template-columns: 132px minmax(0, 1fr); gap: var(--sp-3); }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); overflow-wrap: anywhere; }
  @media (max-width: 400px) { .computer div { grid-template-columns: 1fr; gap: var(--sp-1); } }
</style>
