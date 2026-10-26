<script lang="ts">
  // The run dock: a small panel pinned above the status bar that shows the analyzer run in
  // progress (each analyzer's live step, elapsed time, cancel) and a summary when it ends. It
  // lives at the app root, so it stays put across tabs and dialogs and never blocks the UI.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import { securityRuns as r } from "../../lib/security-runs.svelte";
  import { elapsed, runFraction, stepLabel } from "../../lib/security-view";
  import { scheduler } from "../../lib/schedule-driver.svelte";
  import { untilLabel } from "../../lib/schedule";

  let { onresults }: { onresults?: () => void } = $props();
  let now = $state(Date.now());
  $effect(() => {
    if (!r.busy) return;
    const t = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(t);
  });
  const p = $derived(r.progress);
  const took = $derived(r.startedAt ? elapsed((r.busy ? now : r.finishedAt ?? now) - r.startedAt) : "");
  const pct = $derived(Math.round(runFraction(p) * 100));
  const title = $derived(r.busy ? `Scanning · ${Math.min(p.done + 1, p.total)} of ${p.total}` : p.phase === "cancelled" ? "Scan cancelled" : p.phase === "failed" ? "Scan finished with errors" : "Scan finished");
</script>

{#if r.visible}
  <section class="dock ph-{p.phase}" class:collapsed={r.collapsed} aria-label="Analyzer run" data-run-dock>
    <header>
      <span class="t" role="status" aria-live="polite">
        {#if r.busy}<span class="spin" aria-hidden="true"></span>{:else}<Icon name={p.phase === "done" ? "check" : p.phase === "cancelled" ? "x" : "alert"} size={13} />{/if}
        <span>{title}</span><span class="time" aria-label="Elapsed">{took}</span>
      </span>
      <span class="hacts">
        <IconButton icon={r.collapsed ? "chevron-up" : "chevron-down"} label={r.collapsed ? "Show run details" : "Hide run details"} size="xs" onclick={() => (r.collapsed = !r.collapsed)} />
        {#if !r.busy}<IconButton icon="x" label="Dismiss" size="xs" onclick={() => r.clear()} />{/if}
      </span>
    </header>
    <span class="bar" role="progressbar" aria-label="Run progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}><i style="width:{pct}%"></i></span>
    {#if !r.collapsed}
      <ol class="steps">
        {#each p.steps as s (s.id)}
          <li class="st-{s.phase}" class:empty={s.phase === "done" && s.subjects === 0}>
            <span class="dot" aria-hidden="true"></span>
            <span class="nm">{s.name}</span>
            <span class="lbl">{stepLabel(s)}</span>
            {#if s.phase === "running" && s.event?.agents.length}<span class="ag">for {s.event.agents.join(", ")}</span>{/if}
          </li>
        {/each}
      </ol>
      <footer>
        {#if r.busy}
          <span class="sum">Runs in the background. You can keep working.</span>
          <Button size="xs" variant="danger-outline" icon="stop" onclick={() => r.cancel()}>Cancel</Button>
        {:else}
          <span class="sum">{r.summary}{#if scheduler.driver?.enabled && scheduler.driver.nextMs} · Next scheduled scan {untilLabel(scheduler.driver.nextMs, Date.now())}{/if}</span>
          {#if onresults}<Button size="xs" variant="secondary" icon="list" onclick={onresults}>View results</Button>{/if}
        {/if}
      </footer>
    {/if}
  </section>
{/if}

<style>
  .dock { position: fixed; right: 16px; bottom: 44px; z-index: 40; width: min(380px, calc(100vw - 32px)); display: grid; gap: 8px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); box-shadow: var(--shadow-md); font-size: var(--fs-body-sm); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .t { display: inline-flex; align-items: center; gap: 8px; font-weight: var(--fw-semibold); min-width: 0; }
  .time { color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .ph-done .t { color: var(--ok); } .ph-failed .t { color: var(--danger); }
  .hacts { display: inline-flex; gap: 2px; }
  .bar { height: 3px; border-radius: 2px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); transition: width 200ms; }
  .steps { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; max-height: 200px; overflow: auto; }
  .steps li { display: grid; grid-template-columns: 8px auto minmax(0, 1fr); gap: 4px 8px; align-items: baseline; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--border-strong); align-self: center; }
  .st-running .dot { background: var(--accent); } .st-done .dot { background: var(--ok); } .st-failed .dot { background: var(--danger); }
  .nm { font-weight: var(--fw-medium); white-space: nowrap; }
  .lbl { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st-failed .lbl { color: var(--danger); white-space: normal; }
  .empty .dot { background: var(--border-strong); } .empty .lbl { color: var(--muted); white-space: normal; }
  .ag { grid-column: 3; color: var(--muted); font-size: var(--fs-caption); }
  footer { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-top: 8px; border-top: 1px solid var(--border); }
  .sum { color: var(--muted); font-size: var(--fs-caption); }
  .spin { width: 11px; height: 11px; border: 2px solid var(--accent-soft); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } .bar i { transition: none; } }
</style>
