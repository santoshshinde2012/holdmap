<script lang="ts" module>
  export type Phase = "confirm" | "running" | "done" | "failed";
</script>

<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import Icon from "./Icon.svelte";
  import FrameworkIcon from "./FrameworkIcon.svelte";
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
  } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  let dialog: HTMLDivElement | undefined = $state();
  let confirmBtn: HTMLButtonElement | undefined = $state();
  const force = $derived(isForce(plan));
  const blocked = $derived(plan.blocked);
  const running = $derived(phase === "running");
  const grace = $derived.by(() => {
    const s = plan.steps.find((x) => x.action === "signal_processes");
    return s && s.action === "signal_processes" && !s.force ? s.timeout_ms : 0;
  });
  const escalated = $derived(log.some((l) => /SIGKILL/.test(l) && /send|sent/.test(l)));
  const freed = $derived(phase === "done" || log.some((l) => /is free/.test(l)));
  const verb = $derived(cluster ? "Stop cluster" : force ? "Force kill" : entry?.container ? "Stop container" : "Stop");
  const order = $derived(cluster ? orderFromSummary(plan.summary) : []);
  const subject = $derived(cluster ? cluster.name : entry ? title(entry) : plan.target);

  function stepState(i: number): "done" | "active" | "pending" {
    if (phase === "done") return "done";
    if (phase !== "running") return "pending";
    const last = plan.steps.length - 1;
    if (i < last) return freed || log.length > 1 ? "done" : "active";
    return freed ? "done" : log.length > 0 ? "active" : "pending";
  }

  $effect(() => {
    if (phase === "confirm") (confirmBtn ?? dialog?.querySelector<HTMLButtonElement>("button"))?.focus();
  });

  function trap(e: KeyboardEvent) {
    if (e.key !== "Tab" || !dialog) return;
    const items = [...dialog.querySelectorAll<HTMLElement>("button:not([disabled])")];
    if (!items.length) return;
    const first = items[0], last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) { last.focus(); e.preventDefault(); }
    else if (!e.shiftKey && document.activeElement === last) { first.focus(); e.preventDefault(); }
  }
</script>

<div class="backdrop" role="presentation" transition:fade={{ duration: reduced ? 0 : 120 }} onclick={() => phase === "confirm" && oncancel()}>
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-summary"
    aria-busy={running}
    tabindex="-1"
    bind:this={dialog}
    in:fly={{ y: reduced ? 0 : 8, duration: reduced ? 0 : 180 }}
    onclick={(e) => e.stopPropagation()}
    onkeydown={trap}
  >
    {#if phase === "done" && report}
      <div class="result" in:fade={{ duration: reduced ? 0 : 150 }} role="status">
        <div class="big-check"><Icon name="check" size={26} /></div>
        <h2 id="confirm-title">{cluster ? `Cluster ${cluster.name} stopped` : `Port ${entry?.port} is free`}</h2>
        <p id="confirm-summary">{subject} stopped in {seconds(report.elapsed_ms)}{report.escalated ? " — it ignored SIGTERM, so portwise used SIGKILL" : ""}.</p>
      </div>
    {:else}
      <div class="top">
        {#if entry && !cluster}<FrameworkIcon {entry} size={40} />{:else}<span class="ctile"><Icon name="layers" size={20} /></span>{/if}
        <div class="tt">
          <h2 id="confirm-title">
            {#if blocked}Can't stop {cluster ? cluster.name : `:${entry?.port}`} safely{:else}{verb} {subject}?{/if}
          </h2>
          {#if cluster}
            <div class="sub">{CLUSTER_LABEL[cluster.kind]}{cluster.detail ? ` · ${cluster.detail}` : ""} · {cluster.nodes.length} services</div>
          {:else if entry}
            <div class="sub mono">:{entry.port} · {entry.process ? `${entry.process.name} · PID ${entry.process.pid}` : entry.container?.name ?? ""}</div>
          {/if}
        </div>
        {#if !blocked}<span class="badge risk-{plan.risk}">{plan.risk} risk</span>{/if}
      </div>
      <p id="confirm-summary" class="summary selectable">{blocked ? blocked.message : plan.summary}</p>

      {#if order.length > 1 && !blocked}
        <div class="order" aria-label="Stop order">
          <span class="ol">Order</span>
          {#each order as o, i}{#if i > 0}<span class="arr" aria-hidden="true">→</span>{/if}<span class="chipo mono">{i + 1}. {o}</span>{/each}
        </div>
      {/if}
      {#if !blocked}
        <ol class="steps" aria-label="Plan">
          {#each plan.steps as s, i}
            {@const st = stepState(i)}
            <li class={st}>
              <span class="n" aria-hidden="true">
                {#if st === "done"}<Icon name="check" size={11} />{:else if st === "active"}<span class="spin"><Icon name="refresh" size={11} /></span>{:else}{i + 1}{/if}
              </span>
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
        {#each plan.warnings as w}<div class="warning"><Icon name="alert" size={14} />{w}</div>{/each}
        {#if plan.risk !== "low" && phase === "confirm" && !(cluster && plan.risk === "medium")}
          <div class="warning strong"><Icon name="alert" size={14} />{plan.risk === "high" ? "High risk: this isn't one of your dev servers." : "This isn't a dev server — make sure nothing needs it."}</div>
        {/if}
        {#if phase === "failed" && report}
          <div class="warning strong" role="alert"><Icon name="alert" size={14} />{report.error ?? `Port still busy: ${report.ports_still_busy.join(", ")}`}</div>
        {/if}
      {/if}

      {#if log.length}
        <details class="log" open={phase === "failed"}>
          <summary>Activity ({log.length})</summary>
          <div class="mono" aria-live="polite">{#each log as line}<div>› {line}</div>{/each}</div>
        </details>
      {/if}

      <footer>
        {#if blocked}
          {#if canOverride(plan)}<button class="btn danger-ghost" onclick={onoverride}>I understand — stop anyway</button>{/if}
          <button class="btn primary" onclick={oncancel} bind:this={confirmBtn}>Got it</button>
        {:else if phase === "failed"}
          <button class="btn primary" onclick={oncancel} bind:this={confirmBtn}>Close</button>
        {:else}
          <span class="hint">Nothing is sent until you confirm.</span>
          <button class="btn" onclick={oncancel} disabled={running}>Cancel <kbd>Esc</kbd></button>
          <button class="btn danger" onclick={onconfirm} disabled={running} bind:this={confirmBtn}>
            {#if running}<span class="spin"><Icon name="refresh" size={13} /></span>Stopping…{:else}{verb} <kbd>↵</kbd>{/if}
          </button>
        {/if}
      </footer>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: var(--backdrop); backdrop-filter: blur(4px) saturate(120%); display: grid; place-items: center; z-index: 50; padding: var(--sp-4); }
  .dialog { width: min(540px, 100%); background: var(--surface); border-radius: var(--r-xl); box-shadow: var(--shadow-lg); padding: var(--sp-6); outline: none; }
  .top { display: flex; gap: var(--sp-3); align-items: center; margin-bottom: var(--sp-3); }
  .tt { flex: 1; min-width: 0; }
  h2 { margin: 0; font-size: var(--fs-lg); letter-spacing: -0.015em; font-weight: 650; }
  .sub { color: var(--muted); font-size: 11.5px; margin-top: 2px; }
  .summary { margin: 0 0 var(--sp-4); color: var(--text-2); }
  .steps { list-style: none; margin: 0 0 var(--sp-3); padding: var(--sp-3); display: grid; gap: 10px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .steps li { display: flex; gap: 10px; align-items: flex-start; color: var(--text-2); font-size: var(--fs-sm); transition: color var(--dur-2); }
  .steps li.done { color: var(--text); }
  .n { flex: none; width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 10.5px; font-weight: 700; background: var(--surface); color: var(--muted); border: 1px solid var(--border-strong); transition: background var(--dur-2), color var(--dur-2); }
  .done .n { background: var(--ok); border-color: transparent; color: #fff; }
  .active .n { background: var(--accent-soft); border-color: transparent; color: var(--accent); }
  .grace { display: grid; gap: 6px; font-size: var(--fs-xs); color: var(--muted); margin: 0 0 var(--sp-3); }
  .bar { height: 4px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .fill { height: 100%; width: 0; background: var(--accent); border-radius: 4px; animation: fill var(--dur) linear forwards; }
  .fill.esc { background: var(--danger); width: 100%; animation: none; }
  @keyframes fill { to { width: 100%; } }
  .warning { display: flex; gap: 8px; align-items: center; color: var(--warn); font-size: var(--fs-sm); margin: 6px 0; }
  .warning.strong { font-weight: 600; }
  .log { margin-top: var(--sp-2); font-size: var(--fs-xs); color: var(--muted); }
  .log summary { cursor: pointer; width: max-content; }
  .log .mono { max-height: 120px; overflow-y: auto; font-size: 11px; background: var(--surface-2); color: var(--text-2); border-radius: var(--r-sm); padding: 8px 10px; margin-top: 6px; display: grid; gap: 2px; }
  footer { display: flex; justify-content: flex-end; align-items: center; gap: var(--sp-2); margin-top: var(--sp-5); }
  .hint { margin-right: auto; color: var(--muted); font-size: var(--fs-xs); }
  .result { text-align: center; padding: var(--sp-4) 0 var(--sp-2); }
  .result p { color: var(--muted); margin: var(--sp-2) 0 0; }
  .big-check { width: 56px; height: 56px; margin: 0 auto var(--sp-4); border-radius: 50%; display: grid; place-items: center; background: var(--ok-soft); color: var(--ok); animation: pop 360ms var(--ease-spring); }
  @keyframes pop { from { transform: scale(0.5); opacity: 0; } }
  .ctile { width: 40px; height: 40px; border-radius: 10px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); flex: none; }
  .order { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin: 0 0 var(--sp-3); }
  .ol { font-size: var(--fs-2xs); text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); font-weight: 700; margin-right: 4px; }
  .chipo { font-size: 11px; padding: 3px 8px; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--border); color: var(--text); }
  .arr { color: var(--muted); font-weight: 700; }
  .risk-low { color: var(--ok); background: var(--ok-soft); }
  .risk-medium { color: var(--warn); background: var(--warn-soft); }
  .risk-high { color: var(--danger); background: var(--danger-soft); }
  @media (max-width: 520px) { .hint { display: none; } .dialog { padding: var(--sp-4); } }
</style>
