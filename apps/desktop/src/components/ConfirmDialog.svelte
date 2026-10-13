<script lang="ts" module>
  export type Phase = "confirm" | "running" | "done" | "failed";
</script>

<script lang="ts">
  // Stop confirmation: the exact plan, live progress, success/failure, and the explicit
  // "I understand" checkbox before overriding a soft-protected process.
  import RichText from "./ui/RichText.svelte";
  import { untrack } from "svelte";
  import { fade } from "svelte/transition";
  import Icon from "./Icon.svelte";
  import FrameworkIcon from "./FrameworkIcon.svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Checkbox from "./ui/Checkbox.svelte";
  import Callout from "./ui/Callout.svelte";
  import ActionTiming from "./ActionTiming.svelte";
  import { resolveScheduleDelay, scheduleDeadline, type ScheduledAction } from "../lib/schedules";
  import type { ActionPlan, Cluster, PortEntry, StopReport } from "../lib/types";
  import { CLUSTER_LABEL, orderFromSummary } from "../lib/graph";
  import { canOverride, describeStep, isForce, seconds, title } from "../lib/format";

  let {
    entry,
    plan,
    phase,
    log,
    report,
    onconfirm,
    oncancel,
    onoverride,
    cluster = null,
    restart = false,
    portCount = 1,
    completedPortCount,
    attemptedPortCount,
    agentName = null,
    onschedule,
    schedulingBusy = false,
    schedulingError = null,
    scheduled = null,
    onviewschedules,
    onreview,
    demo = false,
    intent = "combined",
    oncopyrestart,
  }: {
    /** The port being stopped; null when stopping a whole cluster. */
    entry: PortEntry | null;
    /** Set when stopping a cluster in dependency order. */
    cluster?: Cluster | null;
    plan: ActionPlan;
    phase: Phase;
    log: string[];
    report: StopReport | null;
    onconfirm: () => void;
    oncancel: () => void;
    onoverride: () => void;
    /** Stop, then start the same command again (the details pane's Restart). */
    restart?: boolean;
    /** Number of reviewed ports in an agent bulk action; single-port dialogs keep their labels. */
    portCount?: number;
    /** Actual freed ports, including a partially completed bulk stop. */
    completedPortCount?: number;
    attemptedPortCount?: number;
    agentName?: string | null;
    onschedule?: (delaySecs: number) => void;
    schedulingBusy?: boolean;
    schedulingError?: string | null;
    scheduled?: ScheduledAction | null;
    onviewschedules?: () => void;
    onreview?: () => void;
    demo?: boolean;
    /** Separate entry points cannot switch between immediate and scheduled execution. */
    intent?: "combined" | "now" | "schedule";
    oncopyrestart?: () => void;
  } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  let understood = $state(false);
  let delaySecs = $state(untrack(() => intent === "schedule" ? 3600 : 0));
  let atMs = $state<number | null>(null);
  let timingError = $state<string | null>(null);
  let timingValid = $state(true);
  let scheduleSubmitted = $state(false);
  $effect(() => { void plan.confirmation_id; scheduleSubmitted = false; });
  $effect(() => { void delaySecs; void atMs; timingError = null; });
  const force = $derived(isForce(plan));
  const blocked = $derived(plan.blocked);
  const overridable = $derived(intent !== "schedule" && !!blocked && canOverride(plan));
  const running = $derived(phase === "running");
  const busy = $derived(running || schedulingBusy);
  const canSchedule = $derived(intent !== "now" && !!onschedule && !!entry && !cluster && !restart && !force && !plan.allow_protected && !blocked);
  const scheduling = $derived(intent === "schedule" || (canSchedule && (atMs !== null || delaySecs !== 0)));
  const grace = $derived.by(() => {
    const s = plan.steps.find((x) => x.action === "signal_processes");
    return s && s.action === "signal_processes" && !s.force ? s.timeout_ms : 0;
  });
  const escalated = $derived(log.some((l) => /SIGKILL/.test(l) && /send|sent/.test(l)));
  const freed = $derived(phase === "done" || log.some((l) => /is free/.test(l)));
  /** Neither a port nor a cluster: a bulk stop ("all dev servers"). */
  const bulk = $derived(!entry && !cluster);
  const multiple = $derived(!!entry && !cluster && Number.isSafeInteger(portCount) && portCount > 1);
  const verb = $derived(restart ? "Restart" : cluster ? "Stop cluster" : force ? "Force kill" : entry?.container ? "Stop container" : "Stop");
  /** The confirm button names its target ("Stop :3000"), so the click is unambiguous on its own. */
  const confirmLabel = $derived(multiple ? `Stop ${portCount} ports` : entry ? `${verb} :${entry.port}` : bulk ? "Stop all" : verb);
  const order = $derived(cluster ? orderFromSummary(plan.summary) : []);
  const subject = $derived(multiple ? `${agentName ? `${agentName}'s ` : ""}${portCount} ports` : cluster ? cluster.name : entry ? title(entry) : plan.target);
  const completedCount = $derived(completedPortCount ?? portCount);
  const resultSubject = $derived(multiple ? `${completedCount} port${completedCount === 1 ? "" : "s"}` : subject);
  const skippedCount = $derived(multiple ? Math.max(0, portCount - (attemptedPortCount ?? completedCount)) : 0);
  const heading = $derived(scheduled ? "Stop scheduled" : phase === "done" ? (multiple && completedCount > 1 ? `${completedCount} ports are free` : cluster ? `Cluster ${cluster.name} stopped` : bulk ? "Dev servers stopped" : `Port ${entry?.port} is free`) : blocked ? (multiple ? `Can't stop ${subject} safely` : bulk ? `Nothing to stop` : `Can't stop ${cluster ? cluster.name : `:${entry?.port}`} safely`) : intent === "schedule" ? `Schedule stop :${entry?.port}?` : `${verb} ${subject}?`);
  const sub = $derived(multiple ? "Every port's steps appear below" : cluster ? `${CLUSTER_LABEL[cluster.kind]}${cluster.detail ? ` · ${cluster.detail}` : ""} · ${cluster.nodes.length} services` : entry ? `:${entry.port} · ${entry.process ? `${entry.process.name} · PID ${entry.process.pid}` : entry.container?.name ?? ""}` : bulk ? stepCount(plan) : "");

  function stepCount(p: ActionPlan): string {
    const s = p.steps.find((x) => x.action === "signal_processes");
    const n = s && s.action === "signal_processes" ? s.processes.length : 0;
    return n ? `${n} process${n === 1 ? "" : "es"} · yours, not protected` : "";
  }

  function stepState(i: number): "done" | "active" | "pending" {
    if (phase === "done") return "done";
    if (phase !== "running") return "pending";
    const last = plan.steps.length - 1;
    if (i < last) return freed || log.length > 1 ? "done" : "active";
    return freed ? "done" : log.length > 0 ? "active" : "pending";
  }
  function confirm() {
    if (phase !== "confirm" || blocked || busy || (intent === "schedule" && !canSchedule) || !timingValid || schedulingError || timingError || scheduleSubmitted || scheduled) return;
    if (scheduling) {
      try {
        const resolved = resolveScheduleDelay(delaySecs, atMs);
        scheduleSubmitted = true;
        onschedule?.(resolved);
      } catch (failure) { timingError = String(failure); }
    } else onconfirm();
  }
</script>

<Dialog role="alertdialog" size="md" label={heading} dismissable={!busy} onclose={oncancel} initialFocus="[data-primary]">
  {#snippet header()}
    {#if phase !== "done"}
      <header class="top">
        {#if entry && !cluster}<FrameworkIcon {entry} size={40} />{:else}<span class="ctile"><Icon name="layers" size={20} /></span>{/if}
        <div class="tt">
          <h2>{heading}</h2>
          {#if sub}<div class="sub" class:mono={!cluster}>{sub}</div>{/if}
        </div>
        {#if !blocked}<span class="badge risk-{plan.risk}">{plan.risk} risk</span>{/if}
      </header>
    {/if}
  {/snippet}

  {#if scheduled}
    <div role="status"><Callout tone="ok" title={demo ? "Simulation scheduled" : "Stop scheduled"}>Scheduled for {scheduleDeadline(scheduled.due_at_ms)}. {demo ? "Your agents and ports will not be changed." : "Only the reviewed services will be stopped. Their identities will be checked again. Keep Holdmap running and this computer awake."}</Callout></div>
  {:else if phase === "done" && report}
    <div class="result" in:fade={{ duration: reduced ? 0 : 150 }} role="status">
      <div class="big-check"><Icon name="check" size={26} /></div>
      <h2>{heading}</h2>
      <p>{resultSubject} stopped in {seconds(report.elapsed_ms)}{report.escalated ? " — it ignored SIGTERM, so holdmap used SIGKILL" : ""}.</p>
    </div>
  {:else}
    <p class="summary selectable"><RichText text={blocked ? blocked.message : plan.summary} /></p>
    {#if restart && !blocked}<p class="summary restart-note">Then holdmap starts the same command again in {entry?.project?.root ?? entry?.process?.cwd ?? "its folder"}.</p>{/if}

    {#if blocked && overridable}
      <div class="override">
        <Callout tone="danger" icon="shield" title="This process is protected">It looks like part of your editor, terminal or an agent session. Stopping it can close windows or lose unsaved work.</Callout>
        <Checkbox tone="danger" bind:checked={understood} label="I understand — stop it anyway" description="holdmap will plan the stop without protection and show you the steps first." />
      </div>
    {/if}

    {#if order.length > 1 && !blocked}
      <div class="order" aria-label="Stop order">
        <span class="ol">Order</span>
        {#each order as o, i}{#if i > 0}<Icon name="arrow" size={12} />{/if}<span class="chipo">{i + 1}. {o}</span>{/each}
      </div>
    {/if}
    {#if !blocked}
      <ol class="steps" aria-label="Plan">
        {#each plan.steps as s, i}
          {@const st = stepState(i)}
          <li class={st}>
            <span class="n" aria-hidden="true">{#if st === "done"}<Icon name="check" size={11} />{:else if st === "active"}<span class="spin"><Icon name="refresh" size={11} /></span>{:else}{i + 1}{/if}</span>
            <span>{describeStep(s)}</span>
            <span class="sr-only">{st}</span>
          </li>
        {/each}
      </ol>
      {#if canSchedule && phase === "confirm"}<ActionTiming allowNow={intent !== "schedule"} initialDelaySecs={intent === "schedule" ? 3600 : 0} bind:delaySecs bind:atMs bind:valid={timingValid} {demo} disabled={busy || scheduleSubmitted || !!schedulingError} />{/if}
      {#if running && grace && !freed}
        <div class="grace" aria-hidden="true">
          <div class="bar"><div class="fill" class:esc={escalated} style="--dur: {grace}ms"></div></div>
          <span>{escalated ? "Still running — sent SIGKILL" : `Waiting up to ${seconds(grace)} for a graceful exit…`}</span>
        </div>
      {/if}
      <div class="notes">
        {#each plan.warnings as w}<Callout tone="warn" size="sm">{w}</Callout>{/each}
        {#if plan.risk !== "low" && phase === "confirm" && !((cluster || bulk) && plan.risk === "medium")}
          <Callout tone={plan.risk === "high" ? "danger" : "warn"} size="sm">{plan.risk === "high" ? "High risk: this isn't one of your dev servers." : "This isn't a dev server — make sure nothing needs it."}</Callout>
        {/if}
        {#if phase === "failed" && report}
          <Callout tone="danger" size="sm">{report.error ?? (report.freed ? "The port is free, but the reviewed stop didn't complete successfully." : `Port still busy: ${report.ports_still_busy.join(", ")}`)}</Callout>
          {#if report.freed}<Callout tone="warn" size="sm">The port is now free. Further stops and automatic restart were skipped because a reviewed step failed.</Callout>{/if}
        {/if}
        {#if schedulingError}<Callout tone="danger" title="Could not schedule the stop">{schedulingError}</Callout>{/if}
        {#if timingError}<Callout tone="danger" title="Choose a new scheduled time">{timingError}</Callout>{/if}
      </div>
    {/if}

    {#if log.length}
      <details class="log" open={phase === "failed"}>
        <summary>Activity ({log.length})</summary>
        <div class="mono" aria-live="polite">{#each log as line}<div>› {line}</div>{/each}</div>
      </details>
    {/if}
  {/if}

  {#if multiple && (phase === "failed" || report?.history_error)}
    <Callout tone="warn" title="Batch outcome">
      <p>{completedCount} of {portCount} ports confirmed free.</p>
      <p>{attemptedPortCount ?? completedCount} attempted · {skippedCount} not attempted.</p>
    </Callout>
  {/if}
  {#if report?.history_error}
    <Callout tone="danger" title="Restart history couldn't be saved">
      <p>{report.history_error}</p>
      {#if restart}<p>Automatic restart was skipped. Start the service again with its normal launcher.</p>{/if}
      {#if skippedCount}<p>The remaining {skippedCount} reviewed {skippedCount === 1 ? "port was" : "ports were"} not stopped.</p>{/if}
    </Callout>
  {/if}

  {#snippet footer()}
    {#if scheduled}
      <Button variant="secondary" onclick={oncancel} data-primary>Done</Button>
      {#if onviewschedules}<Button variant="primary" icon="clock" onclick={onviewschedules}>View scheduled actions</Button>{/if}
    {:else if phase === "done"}
      <Button variant="secondary" onclick={oncancel} data-primary>Done</Button>
      {#if report?.history_error && oncopyrestart}<Button variant="primary" icon="copy" onclick={oncopyrestart}>Copy restart command</Button>{/if}
    {:else if blocked}
      <span class="hint">{overridable ? "Nothing is sent yet." : "holdmap won't stop this for you."}</span>
      {#if overridable}
        <Button variant="secondary" kbd="Esc" onclick={oncancel}>Cancel</Button>
        <Button variant="danger" icon="stop" disabled={!understood} onclick={onoverride} data-primary={understood || undefined}>Review stop</Button>
      {:else}
        <Button variant="primary" onclick={oncancel} data-primary>Got it</Button>
      {/if}
    {:else if phase === "failed"}
      {#if onreview}<Button variant="secondary" onclick={onreview}>Review again</Button>{/if}
      <Button variant="primary" onclick={oncancel} data-primary>Close</Button>
    {:else}
      <span class="hint">{scheduling ? "Nothing stops until the scheduled time." : "Nothing is sent until you confirm."}</span>
      <Button variant="secondary" kbd="Esc" onclick={oncancel} disabled={busy}>Cancel</Button>
      {#if schedulingError && onreview}<Button variant="secondary" onclick={onreview}>Review again</Button>{/if}
      <Button variant="danger" icon={scheduling ? "clock" : "stop"} kbd="↵" loading={busy} loadingText={schedulingBusy ? "Scheduling…" : "Stopping…"} disabled={(intent === "schedule" && !canSchedule) || !timingValid || !!timingError || !!schedulingError || scheduleSubmitted} onclick={confirm} data-primary>{scheduling ? demo ? "Schedule simulation" : multiple ? `Schedule ${portCount} ports` : `Schedule stop :${entry?.port}` : confirmLabel}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .top { display: flex; gap: var(--sp-3); align-items: center; padding: var(--sp-5) var(--sp-5) var(--sp-3) var(--sp-6); }
  .tt { flex: 1; min-width: 0; }
  h2 { margin: 0; font-size: var(--fs-title); letter-spacing: var(--ls-title); font-weight: var(--fw-semibold); line-height: var(--lh-title); }
  .sub { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 3px; }
  .sub.mono { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .ctile { width: 40px; height: 40px; border-radius: var(--r-md); display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); flex: none; }
  .summary { margin: 0 0 var(--sp-4); color: var(--text-2); }
  .override { display: grid; gap: var(--sp-4); margin-bottom: var(--sp-2); }
  .steps { list-style: none; margin: 0 0 var(--sp-3); padding: 12px; display: grid; gap: 10px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-lg); }
  .steps li { display: flex; gap: 10px; align-items: flex-start; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); transition: color var(--dur-2); }
  .steps li.done { color: var(--text); }
  .n { flex: none; width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); background: var(--surface); color: var(--muted); border: 1px solid var(--border-strong); transition: background var(--dur-2), color var(--dur-2); }
  .done .n { background: var(--ok); border-color: transparent; color: #fff; }
  .active .n { background: var(--accent-soft); border-color: transparent; color: var(--accent); }
  .grace { display: grid; gap: 6px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); margin: 0 0 var(--sp-3); }
  .bar { height: 4px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .fill { height: 100%; width: 0; background: var(--accent); border-radius: 4px; animation: fill var(--dur) linear forwards; }
  .fill.esc { background: var(--danger); width: 100%; animation: none; }
  @keyframes fill { to { width: 100%; } }
  .notes { display: grid; gap: 6px; }
  .notes:empty { display: none; }
  .log { margin-top: var(--sp-3); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .log summary { cursor: pointer; width: max-content; border-radius: var(--r-xs); }
  .log .mono { max-height: 120px; overflow-y: auto; font-size: var(--fs-caption); line-height: var(--lh-caption); background: var(--surface-2); color: var(--text-2); border-radius: var(--r-md); padding: 8px 10px; margin-top: 6px; display: grid; gap: 2px; }
  .hint { margin-right: auto; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .result { text-align: center; padding: var(--sp-6) 0 var(--sp-2); }
  .result p { color: var(--muted); margin: var(--sp-2) 0 0; }
  .big-check { width: 56px; height: 56px; margin: 0 auto var(--sp-4); border-radius: 50%; display: grid; place-items: center; background: var(--ok-soft); color: var(--ok); animation: pop 360ms var(--ease-spring); }
  @keyframes pop { from { transform: scale(0.5); opacity: 0; } }
  .order { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin: 0 0 var(--sp-3); color: var(--muted); }
  .ol { font-size: var(--fs-label); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); font-weight: var(--fw-medium); margin-right: 4px; }
  .chipo { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); padding: 3px 8px; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--border); color: var(--text); }
  .risk-low { color: var(--ok); background: var(--ok-soft); }
  .risk-medium { color: var(--warn); background: var(--warn-soft); }
  .risk-high { color: var(--danger); background: var(--danger-soft); }
  @media (max-width: 560px) { .hint { display: none; } .top { padding: var(--sp-4) var(--sp-4) var(--sp-2); } }
</style>
