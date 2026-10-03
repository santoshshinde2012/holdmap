<script lang="ts">
  import type { Snippet } from "svelte";
  let { tone = "neutral", title, children }: { tone?: "neutral" | "ok" | "err" | "warn"; title: string; children?: Snippet } = $props();
</script>

<div class="state">
  <svg class="art {tone}" width="132" height="96" viewBox="0 0 132 96" aria-hidden="true">
    <circle cx="66" cy="48" r="44" class="r3" />
    <circle cx="66" cy="48" r="30" class="r2" />
    <circle cx="66" cy="48" r="16" class="r1" />
    <path d="M66 48 L102 22" class="sweep" />
    <circle cx="66" cy="48" r="4" class="dot" />
    {#if tone === "ok"}<circle cx="94" cy="66" r="4" class="blip" />{/if}
    {#if tone === "err"}<path d="M60 42l12 12M72 42 60 54" class="x" />{/if}
  </svg>
  <h3>{title}</h3>
  {@render children?.()}
</div>

<style>
  .state { max-width: 400px; margin: 10vh auto 0; text-align: center; color: var(--muted); display: flex; flex-direction: column; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-5); }
  .state h3 { color: var(--text); margin: var(--sp-3) 0 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); line-height: var(--lh-title); letter-spacing: var(--ls-title); }
  .state :global(p) { margin: 0 0 var(--sp-2); }
  .art circle, .art path { fill: none; stroke: var(--border-strong); stroke-width: 1.5; }
  .art .r1 { stroke: var(--accent); opacity: 0.35; }
  .art .sweep { stroke: var(--accent); stroke-linecap: round; transform-origin: 66px 48px; animation: sweep 4s linear infinite; opacity: 0.7; }
  .art .dot { fill: var(--accent); stroke: none; }
  .art .blip { fill: var(--ok); stroke: none; animation: blip 2s ease-in-out infinite; }
  .art.err .sweep, .art.err .r1 { stroke: var(--danger); animation: none; }
  .art.err .dot { fill: var(--danger); }
  .art .x { stroke: var(--danger); stroke-width: 2.5; stroke-linecap: round; }
  .art.warn .r1, .art.warn .sweep { stroke: var(--warn); }
  @keyframes sweep { to { transform: rotate(360deg); } }
  @keyframes blip { 50% { opacity: 0.25; } }
</style>
