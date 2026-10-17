<script lang="ts">
  // Banners under the toolbar: a failed scan (with retry) and the next active scheduled action.
  import Icon from "../Icon.svelte";
  import { Button } from "../ui";
  import { scheduleCountdown, scheduleDeadline, type ScheduledAction } from "../../lib/schedules";

  let { error, retrying, showError, hasSnapshot, next, activeCount, now, demo, onretry, onschedules }: {
    error: string | null;
    retrying: boolean;
    /** The list shows its own full-page error before the first scan. */
    showError: boolean;
    hasSnapshot: boolean;
    next: ScheduledAction | null;
    activeCount: number;
    now: number;
    demo: boolean;
    onretry: () => void;
    onschedules: () => void;
  } = $props();
</script>

<div class="notices">
  {#if error && showError}
    <div class="scan-banner" role="region" aria-label="Scan status">
      <Icon name="alert" size={16} />
      <div class="scan-summary">
        <div role="alert">
          <strong>Couldn't update port status</strong>
          <p class="selectable">{error}</p>
        </div>
        <span>{hasSnapshot ? "Showing the last successful scan. " : ""}Retrying automatically.</span>
      </div>
      <Button size="sm" icon="refresh" loading={retrying} loadingText="Retrying…" onclick={onretry}>Try again</Button>
    </div>
  {/if}
  {#if next}
    <div class="schedule-banner" role="region" aria-label="Active scheduled actions">
      <Icon name="clock" size={16} />
      <div class="schedule-summary">
        <strong>{activeCount} scheduled {activeCount === 1 ? "action" : "actions"}{demo ? " · demo" : ""}</strong>
        <span>{next.label} · {next.status === "running" ? "Running" : next.status === "checking" ? "Checking the reviewed target" : scheduleCountdown(next.due_at_ms, now)} · <time datetime={new Date(next.due_at_ms).toISOString()}>{scheduleDeadline(next.due_at_ms)}</time></span>
      </div>
      <Button size="sm" variant="secondary" onclick={onschedules}>View / cancel</Button>
    </div>
  {/if}
</div>

<style>
  .notices { min-width: 0; }
  .scan-banner { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3) var(--sp-5); background: var(--warn-soft); border-bottom: 1px solid var(--border); color: var(--warn); }
  .scan-summary { display: grid; gap: 3px; flex: 1; min-width: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .scan-summary strong { font-weight: var(--fw-semibold); }
  .scan-summary p { margin: 3px 0 0; color: var(--text-2); overflow-wrap: anywhere; }
  .scan-summary > span { color: var(--text-2); }
  .schedule-banner { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3) var(--sp-5); background: var(--accent-soft); border-bottom: 1px solid var(--border); color: var(--accent); }
  .schedule-summary { display: grid; gap: 3px; flex: 1; min-width: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .schedule-summary span { color: var(--text-2); overflow-wrap: anywhere; }
  @media (max-width: 560px) { .schedule-banner, .scan-banner { padding: var(--sp-2) var(--sp-3); gap: var(--sp-2); } }
</style>
