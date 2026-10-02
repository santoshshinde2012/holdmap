<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { ActionPlan, PortEntry } from "../lib/types";
  import { describeStep, isForce, title } from "../lib/format";

  let {
    entry,
    plan,
    running,
    log,
    onconfirm,
    oncancel,
    onoverride,
  }: {
    entry: PortEntry;
    plan: ActionPlan;
    running: boolean;
    log: string[];
    onconfirm: () => void;
    oncancel: () => void;
    onoverride: () => void;
  } = $props();

  let dialog: HTMLDivElement | undefined = $state();
  let confirmBtn: HTMLButtonElement | undefined = $state();
  const force = $derived(isForce(plan));
  const blocked = $derived(plan.blocked);

  $effect(() => {
    (confirmBtn ?? dialog?.querySelector<HTMLButtonElement>("button"))?.focus();
  });

  // Keep Tab inside the dialog.
  function trap(e: KeyboardEvent) {
    if (e.key !== "Tab" || !dialog) return;
    const items = [...dialog.querySelectorAll<HTMLElement>("button:not([disabled])")];
    if (!items.length) return;
    const first = items[0], last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) { last.focus(); e.preventDefault(); }
    else if (!e.shiftKey && document.activeElement === last) { first.focus(); e.preventDefault(); }
  }
</script>

<div class="backdrop" role="presentation" onclick={() => !running && oncancel()}>
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-summary"
    tabindex="-1"
    bind:this={dialog}
    onclick={(e) => e.stopPropagation()}
    onkeydown={trap}
  >
    <div class="icon" class:danger={!blocked} class:warn={!!blocked}>
      <Icon name={blocked ? "shield" : force ? "zap" : "stop"} size={18} />
    </div>
    <h2 id="confirm-title">
      {#if blocked}Can't stop :{entry.port} safely
      {:else if force}Force-kill {title(entry)} on :{entry.port}?
      {:else}Stop {title(entry)} on :{entry.port}?{/if}
    </h2>
    <p id="confirm-summary" class="summary selectable">{blocked ? blocked.message : plan.summary}</p>

    {#if !blocked}
      <ol class="steps">
        {#each plan.steps as s, i}
          <li><span class="n">{i + 1}</span><span>{describeStep(s)}</span></li>
        {/each}
      </ol>
      {#each plan.warnings as w}
        <div class="warning"><Icon name="alert" size={14} />{w}</div>
      {/each}
      {#if plan.risk !== "low"}
        <div class="warning strong"><Icon name="alert" size={14} />
          {plan.risk === "high" ? "High risk: this isn't one of your dev servers." : "This isn't a dev server — make sure you don't need it."}
        </div>
      {/if}
    {/if}

    {#if running || log.length}
      <div class="log mono" aria-live="polite">
        {#each log as line}<div>› {line}</div>{/each}
        {#if running}<div class="running"><span class="spin"><Icon name="refresh" size={12} /></span> working…</div>{/if}
      </div>
    {/if}

    <footer>
      {#if blocked}
        {#if blocked.kind === "protected"}
          <button class="btn danger-ghost" onclick={onoverride}>I understand, stop it anyway</button>
        {/if}
        <button class="btn primary" onclick={oncancel} bind:this={confirmBtn}>Got it</button>
      {:else}
        <button class="btn" onclick={oncancel} disabled={running}>Cancel <kbd>Esc</kbd></button>
        <button class="btn danger" onclick={onconfirm} disabled={running} bind:this={confirmBtn}>
          {#if running}<span class="spin"><Icon name="refresh" size={13} /></span> Stopping…{:else}{force ? "Force kill" : "Stop"} <kbd class="k">↵</kbd>{/if}
        </button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(8 10 16 / 0.45); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 50; animation: fade 0.15s var(--ease); }
  .dialog { width: min(520px, calc(100vw - 32px)); background: var(--surface); border: 1px solid var(--border); border-radius: 16px; box-shadow: var(--shadow-lg); padding: 22px 22px 18px; animation: pop 0.18s var(--ease); outline: none; }
  .icon { width: 38px; height: 38px; display: grid; place-items: center; border-radius: 11px; margin-bottom: 12px; }
  .icon.danger { background: var(--danger-soft); color: var(--danger); }
  .icon.warn { background: var(--warn-soft); color: var(--warn); }
  h2 { margin: 0 0 6px; font-size: 16px; letter-spacing: -0.01em; }
  .summary { margin: 0 0 14px; color: var(--text-2); }
  .steps { list-style: none; margin: 0 0 12px; padding: 12px; display: grid; gap: 8px; background: var(--surface-2); border-radius: var(--radius); }
  .steps li { display: flex; gap: 10px; align-items: flex-start; color: var(--text-2); font-size: 12.5px; }
  .n { flex: none; width: 19px; height: 19px; border-radius: 50%; display: grid; place-items: center; font-size: 10.5px; font-weight: 700; background: var(--surface); color: var(--muted); border: 1px solid var(--border); }
  .warning { display: flex; gap: 8px; align-items: center; color: var(--warn); font-size: 12.5px; margin: 6px 0; }
  .warning.strong { font-weight: 600; }
  .log { max-height: 130px; overflow-y: auto; font-size: 11.5px; background: var(--surface-2); color: var(--text-2); border-radius: 8px; padding: 8px 10px; margin: 10px 0 0; display: grid; gap: 2px; }
  .running { color: var(--muted); display: flex; gap: 6px; align-items: center; }
  footer { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .k { background: rgb(255 255 255 / 0.18); color: inherit; border-color: transparent; height: 16px; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: translateY(6px) scale(0.98); } }
</style>
