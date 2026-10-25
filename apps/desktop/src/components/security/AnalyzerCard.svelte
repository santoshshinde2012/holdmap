<script lang="ts">
  // One analyzer: initial, name, publisher, what it checks, status, and one primary action.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import { ago } from "../../lib/analyzers";
  import { stepLabel, type AnalyzerCard, type RunStep } from "../../lib/security-view";
  let { card, step = null, busy = false, disabled = false, onaction, onpipeline }: { card: AnalyzerCard; step?: RunStep | null; busy?: boolean; disabled?: boolean; onaction: () => void; onpipeline?: () => void } = $props();
</script>

<li class="card st-{card.status}" aria-labelledby="an-{card.id}">
  <div class="top">
    <span class="logo" aria-hidden="true">{card.initial}</span>
    <span class="id"><b id="an-{card.id}">{card.name}</b><small>{card.publisher}</small></span>
    <span class="status"><i aria-hidden="true"></i>{card.statusLabel}</span>
  </div>
  <p class="checks">{card.checks}</p>
  <div class="tags">
    <span>{card.target}</span>
    {#if card.network.length}<span title="Contacts {card.network.join(', ')}"><Icon name="globe" size={11} />Network</span>{:else}<span><Icon name="lock" size={11} />Offline</span>{/if}
    {#if card.lastRun}<span>{ago(card.lastRun)}{#if card.open} · {card.open} open{/if}</span>{/if}
  </div>
  {#if step}<p class="step ph-{step.phase}" role="status" data-card-step>{stepLabel(step)}</p>{/if}
  <div class="foot">
    <Button size="sm" variant={card.action === "run" ? "primary" : "secondary"} icon={card.action === "run" ? "play" : card.action === "install" ? "download" : undefined} loading={busy} {disabled} onclick={onaction} aria-label="{card.actionLabel.replace('…', '')} {card.name}">{card.actionLabel}</Button>
    {#if onpipeline}<Button size="sm" variant="ghost" icon="branch" onclick={onpipeline} aria-label="Pipeline for {card.name}">Pipeline</Button>{/if}
  </div>
</li>

<style>
  .card { display: grid; grid-template-rows: auto 1fr auto auto auto; gap: 10px; padding: 14px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; }
  .card:hover { border-color: var(--border-strong); }
  .top { display: grid; grid-template-columns: 32px minmax(0, 1fr) auto; align-items: center; gap: 10px; }
  .logo { display: grid; place-items: center; width: 32px; height: 32px; border-radius: var(--r-md); background: var(--surface-3); color: var(--text-2); font-weight: var(--fw-semibold); font-size: var(--fs-body); }
  .st-on .logo { background: var(--accent-soft); color: var(--accent); }
  .id { display: grid; min-width: 0; }
  b { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { color: var(--muted); font-size: var(--fs-caption); }
  .status { display: inline-flex; align-items: center; gap: 5px; color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); white-space: nowrap; }
  .status i { width: 7px; height: 7px; border-radius: 50%; background: var(--border-strong); }
  .st-on .status { color: var(--ok); } .st-on .status i { background: var(--ok); }
  .st-changed .status { color: var(--tone-amber); } .st-changed .status i { background: var(--warn); }
  .checks { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .tags span { display: inline-flex; align-items: center; gap: 4px; height: 20px; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); }
  .step { margin: 0; font-size: var(--fs-caption); color: var(--text-2); }
  .step.ph-running { color: var(--accent); } .step.ph-failed { color: var(--danger); }
  .foot { display: flex; gap: 6px; padding-top: 10px; border-top: 1px solid var(--border); }
</style>
