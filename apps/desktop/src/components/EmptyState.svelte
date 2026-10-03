<script lang="ts">
  // List-area empty / no-match / error state. Same visual language as the details pane
  // placeholder (soft icon tile, title role, muted body) so the two panes read as one app.
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  type Tone = "neutral" | "ok" | "err" | "warn";
  let { tone = "neutral", title, icon, children }: { tone?: Tone; title: string; icon?: string; children?: Snippet } = $props();
  const glyph = $derived(icon ?? ({ neutral: "search", ok: "radar", err: "alert", warn: "alert" } as const)[tone]);
</script>

<div class="state" role="status">
  <div class="art {tone}" aria-hidden="true">
    {#if tone === "ok"}<span class="ring"></span>{/if}
    <span class="core"><Icon name={glyph} size={22} /></span>
  </div>
  <h3>{title}</h3>
  {@render children?.()}
</div>

<style>
  .state { max-width: 380px; margin: 14vh auto 0; text-align: center; color: var(--muted); display: flex; flex-direction: column; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-5); }
  .state h3 { color: var(--text); margin: var(--sp-4) 0 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); line-height: var(--lh-title); letter-spacing: var(--ls-title); }
  .state :global(p) { margin: 0 0 var(--sp-2); text-wrap: pretty; }
  .art { position: relative; width: 64px; height: 64px; display: grid; place-items: center; --c: var(--accent); --c-bg: var(--accent-soft); }
  .art.ok { --c: var(--ok); --c-bg: var(--ok-soft); }
  .art.err { --c: var(--danger); --c-bg: var(--danger-soft); }
  .art.warn { --c: var(--warn); --c-bg: var(--warn-soft); }
  .art.neutral { --c: var(--muted); --c-bg: var(--surface-3); }
  .core { position: relative; width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--c-bg); color: var(--c); }
  .ring { position: absolute; inset: 0; border-radius: 50%; border: 1px solid var(--c); opacity: 0; animation: ping 2.8s var(--ease) infinite; }
  @keyframes ping { 0% { transform: scale(0.6); opacity: 0.5; } 100% { transform: scale(1.2); opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .ring { display: none; } }
</style>
