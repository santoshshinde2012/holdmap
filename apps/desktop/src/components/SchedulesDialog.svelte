<script lang="ts">
  import { onMount } from "svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import Icon from "./Icon.svelte";
  import { activeScheduleCount, scheduleCountdown, scheduleDeadline, SCHEDULE_STATUS, type ScheduledAction } from "../lib/schedules";

  let { schedules, loading = false, error = null, onclose, onrefresh, oncancel, demo = false }: {
    schedules: ScheduledAction[];
    loading?: boolean;
    error?: string | null;
    onclose: () => void;
    onrefresh?: () => void;
    oncancel: (id: string) => Promise<void>;
    demo?: boolean;
  } = $props();

  let now = $state(Date.now());
  let cancelling = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let notice = $state("");
  const activeCount = $derived(activeScheduleCount(schedules));
  const ordered = $derived([...schedules].sort((a, b) => {
    const aActive = ["pending", "checking", "running"].includes(a.status);
    const bActive = ["pending", "checking", "running"].includes(b.status);
    return Number(bActive) - Number(aActive) || (aActive ? a.due_at_ms - b.due_at_ms : b.created_at_ms - a.created_at_ms);
  }));

  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  async function cancel(schedule: ScheduledAction) {
    if (!schedule.cancellable || cancelling[schedule.id]) return;
    cancelling[schedule.id] = true;
    delete errors[schedule.id];
    notice = "";
    try {
      await oncancel(schedule.id);
      notice = `Cancellation confirmed for ${schedule.label}.`;
    } catch (failure) {
      errors[schedule.id] = String(failure);
    } finally {
      cancelling[schedule.id] = false;
    }
  }
</script>

<Dialog title="Scheduled actions" description="Shutdowns and service stops for the computer running Holdmap." icon="clock" size="lg" {onclose} initialFocus="[data-done]">
  <div class="content">
    <Callout tone="info" title={demo ? "Browser simulation" : "While Holdmap is running"}>{demo ? "These are simulations while this demo page stays open. Your computer, agents and ports are never changed." : "Keep Holdmap running, including in the tray, and this computer awake. Quitting Holdmap clears all schedules. Closing the window keeps them active when the tray is available."}</Callout>
    <p class="policy">A missed deadline or clock change requires a new review. Changed process identities also require review. Failed actions are never retried automatically. Agent schedules stop their reviewed services and ports; the agent itself stays running.</p>
    {#if error}<Callout tone="danger" title="Could not load scheduled actions">{error}</Callout>{/if}
    {#if loading && !schedules.length}
      <p role="status" class="empty">Loading scheduled actions…</p>
    {:else if !schedules.length}
      <div class="empty"><Icon name="clock" size={26} /><h3>No scheduled actions</h3><p>Choose a delay when reviewing a computer shutdown or a local port or agent service stop.</p></div>
    {:else}
      <div class="summary">{activeCount} active {activeCount === 1 ? "schedule" : "schedules"}<span>{schedules.length - activeCount} finished</span></div>
      <ul class="schedules" aria-label="Scheduled actions">
        {#each ordered as schedule (schedule.id)}
          <li class="schedule">
            <div class="heading"><span class="tile"><Icon name={schedule.kind === "shutdown" ? "monitor" : schedule.kind === "agent" ? "bot" : "plug"} size={17} /></span><h3>{schedule.label}</h3><span class="state {schedule.status}">{SCHEDULE_STATUS[schedule.status]}</span></div>
            <div class="details">
              <time datetime={new Date(schedule.due_at_ms).toISOString()}>{scheduleDeadline(schedule.due_at_ms)}</time>
              {#if schedule.status === "pending"}<strong class="countdown">{scheduleCountdown(schedule.due_at_ms, now)}</strong>{/if}
            </div>
            {#if schedule.targets.length}<p class="targets">Reviewed ports: {schedule.targets.map((target) => `:${target}`).join(", ")}</p>{/if}
            {#if schedule.message}<p class="message">{schedule.message}</p>{/if}
            {#if schedule.status === "needs_review"}<p class="review">Nothing runs automatically. Start a fresh review from Power settings or the original port or agent.</p>{/if}
            {#if schedule.cancellable}
              <div class="actions"><Button size="sm" variant="secondary" loading={!!cancelling[schedule.id]} loadingText="Cancelling…" onclick={() => cancel(schedule)} aria-label={`Cancel ${schedule.label}`}>Cancel schedule</Button><span>Available until execution begins.</span></div>
            {/if}
            {#if errors[schedule.id]}<Callout tone="danger" size="sm" title="Cancellation was not confirmed">{errors[schedule.id]}</Callout>{/if}
          </li>
        {/each}
      </ul>
    {/if}
    <span class="sr-only" role="status">{notice}</span>
  </div>
  {#snippet footer()}
    {#if onrefresh}<Button variant="secondary" icon="refresh" disabled={loading} onclick={onrefresh}>Refresh</Button>{/if}
    <Button data-done variant="primary" onclick={onclose}>Done</Button>
  {/snippet}
</Dialog>

<style>
  .content { display: grid; gap: var(--sp-4); }
  .policy { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .summary { display: flex; justify-content: space-between; gap: var(--sp-3); color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); }
  .summary span { color: var(--muted); font-weight: var(--fw-regular); }
  .schedules { list-style: none; display: grid; gap: var(--sp-3); margin: 0; padding: 0; }
  .schedule { display: grid; gap: var(--sp-3); padding: var(--sp-4); border: 1px solid var(--border); border-radius: var(--r-lg); }
  .heading { display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; }
  .tile { width: 30px; height: 30px; flex: none; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); border-radius: var(--r-md); }
  h3 { flex: 1; margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); overflow-wrap: anywhere; }
  .state { padding: 3px 8px; border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); }
  .pending, .checking, .running { background: var(--accent-soft); color: var(--accent); }
  .completed { background: var(--ok-soft); color: var(--ok); }
  .needs_review { background: var(--warn-soft); color: var(--warn); }
  .failed { background: var(--danger-soft); color: var(--danger); }
  .details { display: flex; justify-content: space-between; gap: var(--sp-2); flex-wrap: wrap; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); font-variant-numeric: tabular-nums; }
  .countdown { font-weight: var(--fw-medium); color: var(--accent); }
  .targets, .message, .review { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  .targets { font-family: var(--mono); }
  .review { color: var(--warn); }
  .actions { display: flex; align-items: center; flex-wrap: wrap; gap: var(--sp-3); }
  .actions span { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .empty { padding: var(--sp-5) var(--sp-3); display: grid; justify-items: center; gap: var(--sp-3); text-align: center; color: var(--muted); }
  .empty h3 { color: var(--text); }
  .empty p { max-width: 400px; margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); }
</style>
