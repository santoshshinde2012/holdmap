<script lang="ts">
  import { onMount } from "svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import Icon from "./Icon.svelte";
  import { activeScheduleCount, schedulableEntries, scheduleCountdown, scheduleDeadline, SCHEDULE_STATUS, type ScheduledAction } from "../lib/schedules";
  import { title } from "../lib/format";
  import type { PortEntry } from "../lib/types";

  let {
    schedules,
    loading = false,
    error = null,
    onclose,
    onrefresh,
    oncancel,
    demo = false,
    powerAvailable = true,
    entries = [],
    onschedulepower,
    onscheduleport,
  }: {
    schedules: ScheduledAction[];
    loading?: boolean;
    error?: string | null;
    onclose: () => void;
    onrefresh?: () => void;
    oncancel: (id: string) => Promise<void>;
    demo?: boolean;
    powerAvailable?: boolean;
    entries?: PortEntry[];
    onschedulepower?: () => void;
    onscheduleport?: (entry: PortEntry) => void;
  } = $props();

  let now = $state(Date.now());
  let picking = $state(false);
  let cancelling = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let notice = $state("");
  const activeCount = $derived(activeScheduleCount(schedules));
  const ports = $derived(schedulableEntries(entries));
  const canCreate = $derived(!!onschedulepower || !!onscheduleport);
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

<Dialog title="Scheduled actions" description="Shutdowns and service stops for the computer running Holdmap." icon="clock" size="lg" {onclose} initialFocus={picking ? "[data-back]" : canCreate && !schedules.length ? "[data-create]" : "[data-done]"}>
  <div class="content">
    <Callout tone="info" size="sm" title={demo ? "Browser simulation" : "While Holdmap is running"}>{demo ? "These are simulations while this demo page stays open. Your computer, agents and ports are never changed." : "Keep Holdmap running, including in the tray, and this computer awake. Quitting Holdmap clears all schedules. Closing the window keeps them active when the tray is available."}</Callout>
    {#if error}<Callout tone="danger" title="Could not load scheduled actions">{error}</Callout>{/if}

    {#if picking}
      <div class="picker">
        <div class="picker-head">
          <h3>Choose a service to stop later</h3>
          <p>The next step reviews the current owner. Protected processes and remote hosts cannot be scheduled.</p>
        </div>
        {#if ports.length}
          <ul class="ports" aria-label="Services you can schedule">
            {#each ports as entry (entry.id)}
              <li>
                <button type="button" onclick={() => onscheduleport?.(entry)}>
                  <span class="port">:{entry.port}</span>
                  <span class="meta">
                    <strong>{title(entry)}</strong>
                    <small>{entry.framework?.name ?? entry.process?.name ?? entry.container?.name ?? entry.protocol} · {entry.protocol.toUpperCase()}{entry.is_dev ? " · dev" : ""}</small>
                  </span>
                  <Icon name="clock" size={14} />
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <div class="empty compact"><Icon name="plug" size={22} /><h3>No stoppable local services</h3><p>Start a dev server, then schedule it from here or from its details pane.</p></div>
        {/if}
      </div>
    {:else if loading && !schedules.length}
      <p role="status" class="empty">Loading scheduled actions…</p>
    {:else if !schedules.length}
      <div class="empty">
        <h3>No scheduled actions</h3>
        <p>{canCreate ? "Pick what should happen later. Every action is reviewed before it is queued." : "Choose a delay when reviewing a computer shutdown or a local port or agent service stop."}</p>
      </div>
      {#if canCreate}
        <div class="create" role="group" aria-label="Create a schedule">
          {#if onschedulepower}
            <button type="button" class="row" data-create disabled={!powerAvailable} onclick={onschedulepower}>
              <span class="tile"><Icon name="monitor" size={16} /></span>
              <span class="copy">
                <strong>{demo ? "Schedule a shutdown simulation" : "Schedule a computer shutdown"}</strong>
                <span>{demo ? "Review this demo computer, then choose a delay. Nothing is powered off." : "Review this computer, then choose a delay. Keep Holdmap running until then."}</span>
              </span>
              <span class="go"><Icon name="chevron" size={16} /></span>
            </button>
          {/if}
          {#if onscheduleport}
            <button type="button" class="row" onclick={() => (picking = true)}>
              <span class="tile"><Icon name="plug" size={16} /></span>
              <span class="copy">
                <strong>Schedule a service stop</strong>
                <span>Pick a local port, then review its current owner before it is queued.</span>
              </span>
              <span class="go"><Icon name="chevron" size={16} /></span>
            </button>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="summary">{activeCount} active {activeCount === 1 ? "schedule" : "schedules"}<span>{schedules.length - activeCount} finished</span></div>
      {#if canCreate}
        <div class="quick">
          {#if onschedulepower}<Button size="sm" variant="secondary" icon="monitor" disabled={!powerAvailable} onclick={onschedulepower}>{demo ? "Schedule simulation…" : "Schedule shutdown…"}</Button>{/if}
          {#if onscheduleport}<Button size="sm" variant="secondary" icon="plug" onclick={() => (picking = true)}>Schedule a stop…</Button>{/if}
        </div>
      {/if}
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

    <details class="policy">
      <summary>How schedules work</summary>
      <p>A missed deadline or clock change requires a new review. Changed process identities also require review. Failed actions are never retried automatically. Agent schedules stop their reviewed services and ports; the agent itself stays running.</p>
    </details>
    <span class="sr-only" role="status">{notice}</span>
  </div>
  {#snippet footer()}
    {#if picking}<Button data-back variant="secondary" icon="arrow-left" onclick={() => (picking = false)}>Back</Button>
    {:else if onrefresh}<Button variant="secondary" icon="refresh" disabled={loading} onclick={onrefresh}>Refresh</Button>{/if}
    <Button data-done variant="primary" onclick={onclose}>Done</Button>
  {/snippet}
</Dialog>

<style>
  .content { display: grid; gap: var(--sp-4); }
  .policy { color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .policy summary { cursor: pointer; color: var(--text-2); font-weight: var(--fw-medium); }
  .policy p { margin: 8px 0 0; }
  .summary { display: flex; justify-content: space-between; gap: var(--sp-3); color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); }
  .summary span { color: var(--muted); font-weight: var(--fw-regular); }
  .quick { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
  .create { display: grid; gap: 8px; }
  .row { display: flex; align-items: center; gap: 12px; width: 100%; padding: 12px 14px; text-align: left; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); color: var(--text); cursor: pointer; font: inherit; box-shadow: var(--control-shadow); }
  .row:hover:not(:disabled) { border-color: var(--border-strong); background: var(--surface-2); }
  .row:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .row:disabled { opacity: 0.5; cursor: not-allowed; }
  .row .copy { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .row strong { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .row .copy span { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .go { color: var(--muted); flex: none; display: inline-grid; }
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
  .empty { padding: var(--sp-2) 0 var(--sp-1); display: grid; gap: 6px; color: var(--muted); }
  .empty.compact { padding: var(--sp-4) var(--sp-2); justify-items: center; text-align: center; }
  .empty h3 { color: var(--text); }
  .empty p { max-width: 480px; margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); }
  .picker { display: grid; gap: var(--sp-3); }
  .picker-head h3 { margin: 0; }
  .picker-head p { margin: 4px 0 0; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .ports { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; max-height: 280px; overflow: auto; }
  .ports button { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text); cursor: pointer; font: inherit; text-align: left; }
  .ports button:hover { background: var(--surface-2); border-color: var(--border-strong); }
  .ports button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .ports .port { width: 64px; flex: none; font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; color: var(--accent); }
  .ports .meta { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .ports strong { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ports small { color: var(--muted); font-size: var(--fs-caption); }
  .ports :global(svg) { color: var(--muted); flex: none; }
</style>
