<script lang="ts" module>
  export type Phase = "confirm" | "running" | "done" | "failed";
</script>

<script lang="ts">
  // Stop confirmation: the exact plan, live progress, success/failure, and the explicit
  // "I understand" checkbox before overriding a soft-protected process.
  import RichText from "./ui/RichText.svelte";
  import { fade } from "svelte/transition";
  import Icon from "./Icon.svelte";
  import FrameworkIcon from "./FrameworkIcon.svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Checkbox from "./ui/Checkbox.svelte";
  import Callout from "./ui/Callout.svelte";
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
    agentName = null,
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
    agentName?: string | null;
  } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  let understood = $state(false);
  const force = $derived(isForce(plan));
  const blocked = $derived(plan.blocked);
  const overridable = $derived(!!blocked && canOverride(plan));
  const running = $derived(phase === "running");
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
  const heading = $derived(phase === "done" ? (multiple ? `${portCount} ports are free` : cluster ? `Cluster ${cluster.name} stopped` : bulk ? "Dev servers stopped" : `Port ${entry?.port} is free`) : blocked ? (multiple ? `Can't stop ${subject} safely` : bulk ? `Nothing to stop` : `Can't stop ${cluster ? cluster.name : `:${entry?.port}`} safely`) : `${verb} ${subject}?`);
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
</script>

<Dialog role="alertdialog" size="md" label={heading} dismissable={!running} onclose={oncancel} initialFocus="[data-primary]">
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

  {#if phase === "done" && report}
    <div class="result" in:fade={{ duration: reduced ? 0 : 150 }} role="status">
      <div class="big-check"><Icon name="check" size={26} /></div>
      <h2>{heading}</h2>
      <p>{subject} stopped in {seconds(report.elapsed_ms)}{report.escalated ? " — it ignored SIGTERM, so holdmap used SIGKILL" : ""}.</p>
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
        {#if phase === "failed" && report}<Callout tone="danger" size="sm">{report.error ?? `Port still busy: ${report.ports_still_busy.join(", ")}`}</Callout>{/if}
      </div>
    {/if}

    {#if log.length}
      <details class="log" open={phase === "failed"}>
        <summary>Activity ({log.length})</summary>
        <div class="mono" aria-live="polite">{#each log as line}<div>› {line}</div>{/each}</div>
      </details>
    {/if}
  {/if}

  {#snippet footer()}
    {#if phase === "done"}
      <Button variant="secondary" onclick={oncancel} data-primary>Done</Button>
    {:else if blocked}
      <span class="hint">{overridable ? "Nothing is sent yet." : "holdmap won't stop this for you."}</span>
      {#if overridable}
        <Button variant="secondary" kbd="Esc" onclick={oncancel}>Cancel</Button>
        <Button variant="danger" icon="stop" disabled={!understood} onclick={onoverride} data-primary={understood || undefined}>Review stop</Button>
      {:else}
        <Button variant="primary" onclick={oncancel} data-primary>Got it</Button>
      {/if}
    {:else if phase === "failed"}
      <Button variant="primary" onclick={oncancel} data-primary>Close</Button>
    {:else}
      <span class="hint">Nothing is sent until you confirm.</span>
      <Button variant="secondary" kbd="Esc" onclick={oncancel} disabled={running}>Cancel</Button>
      <Button variant="danger" icon="stop" kbd="↵" loading={running} loadingText="Stopping…" onclick={onconfirm} data-primary>{confirmLabel}</Button>
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
