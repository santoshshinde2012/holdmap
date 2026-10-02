<script lang="ts" module>
  export interface Toast { id: number; kind: "ok" | "error" | "info"; text: string; detail?: string }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  let { toasts, ondismiss }: { toasts: Toast[]; ondismiss: (id: number) => void } = $props();
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}">
      <span class="ic"><Icon name={t.kind === "ok" ? "check" : t.kind === "error" ? "alert" : "info"} size={14} /></span>
      <div class="body"><div class="text">{t.text}</div>{#if t.detail}<div class="detail">{t.detail}</div>{/if}</div>
      <button class="icon-btn x" aria-label="Dismiss" onclick={() => ondismiss(t.id)}><Icon name="x" size={13} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; right: 16px; bottom: 16px; display: grid; gap: 8px; z-index: 60; width: min(380px, calc(100vw - 32px)); }
  .toast { display: flex; gap: 10px; align-items: flex-start; padding: 10px 8px 10px 12px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; box-shadow: var(--shadow-lg); animation: slide 0.2s var(--ease); }
  .ic { width: 22px; height: 22px; flex: none; display: grid; place-items: center; border-radius: 50%; }
  .ok .ic { background: var(--ok-soft); color: var(--ok); }
  .error .ic { background: var(--danger-soft); color: var(--danger); }
  .info .ic { background: var(--accent-soft); color: var(--accent); }
  .body { flex: 1; min-width: 0; padding-top: 2px; }
  .text { font-weight: 600; }
  .detail { color: var(--muted); font-size: 12px; margin-top: 2px; overflow-wrap: anywhere; }
  .x { width: 24px; height: 24px; }
  @keyframes slide { from { opacity: 0; transform: translateY(8px); } }
</style>
