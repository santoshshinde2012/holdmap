<script lang="ts" module>
  export interface Toast {
    id: number;
    kind: "ok" | "error" | "info";
    text: string;
    detail?: string;
    action?: { label: string; run: () => void };
  }
</script>

<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import Icon from "./Icon.svelte";
  let { toasts, ondismiss }: { toasts: Toast[]; ondismiss: (id: number) => void } = $props();
  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
</script>

<div class="toasts" role="region" aria-label="Notifications">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"} in:fly={{ y: reduced ? 0 : 12, duration: reduced ? 0 : 200 }} out:fly={{ x: reduced ? 0 : 24, duration: reduced ? 0 : 160 }} animate:flip={{ duration: reduced ? 0 : 180 }}>
      <span class="ic"><Icon name={t.kind === "ok" ? "check" : t.kind === "error" ? "alert" : "info"} size={14} /></span>
      <div class="body">
        <div class="text">{t.text}</div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
        {#if t.action}<button class="act" onclick={() => { t.action?.run(); ondismiss(t.id); }}>{t.action.label}</button>{/if}
      </div>
      <button class="icon-btn x" aria-label="Dismiss notification" onclick={() => ondismiss(t.id)}><Icon name="x" size={13} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: var(--sp-4); bottom: var(--sp-4); display: flex; flex-direction: column; gap: var(--sp-2); z-index: 60; width: min(380px, calc(100vw - 32px)); }
  .toast { display: flex; gap: 10px; align-items: flex-start; padding: 12px 8px 12px 12px; background: var(--surface); border-radius: var(--r-lg); box-shadow: var(--shadow-lg); }
  .ic { width: 24px; height: 24px; flex: none; display: grid; place-items: center; border-radius: 50%; }
  .ok .ic { background: var(--ok-soft); color: var(--ok); }
  .error .ic { background: var(--danger-soft); color: var(--danger); }
  .info .ic { background: var(--accent-soft); color: var(--accent); }
  .body { flex: 1; min-width: 0; padding-top: 2px; }
  .text { font-weight: var(--fw-medium); }
  .detail { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 2px; overflow-wrap: anywhere; }
  .act { margin-top: 8px; border: 0; background: var(--accent-soft); color: var(--accent); font-weight: var(--fw-medium); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); height: 24px; padding: 0 10px; border-radius: var(--r-sm); }
  .act:hover { background: var(--accent); color: var(--accent-fg); }
  .x { width: 24px; height: 24px; }
</style>
