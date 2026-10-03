<script lang="ts">
  // Inline message: tip / info / warn / danger / ok, with icon, optional title and actions.
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  let { tone = "info", icon, title, size = "md", children, actions }: { tone?: "info" | "tip" | "warn" | "danger" | "ok"; icon?: string; title?: string; size?: "sm" | "md"; children: Snippet; actions?: Snippet } = $props();
  const defaults = { info: "info", tip: "sparkles", warn: "alert", danger: "alert", ok: "check" } as const;
</script>

<div class="callout {tone} {size}" role={tone === "danger" ? "alert" : "note"}>
  <span class="ic"><Icon name={icon ?? defaults[tone]} size={size === "sm" ? 13 : 15} /></span>
  <div class="body">
    {#if title}<strong>{title}</strong>{/if}
    <div class="txt">{@render children()}</div>
    {#if actions}<div class="acts">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .callout { display: flex; gap: 10px; padding: 10px 12px; border-radius: var(--r-md); font-size: var(--fs-body); line-height: var(--lh-body); border: 1px solid transparent; }
  .sm { padding: 8px 10px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); gap: 8px; }
  .ic { flex: none; display: inline-grid; margin-top: 2px; }
  .body { min-width: 0; flex: 1; }
  strong { display: block; font-weight: var(--fw-semibold); margin-bottom: 1px; }
  .txt :global(p) { margin: 0; }
  .txt :global(code) { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); padding: 0 4px; border-radius: 4px; background: color-mix(in srgb, currentColor 10%, transparent); }
  .acts { display: flex; gap: 6px; margin-top: 8px; flex-wrap: wrap; }
  .info { background: var(--surface-2); border-color: var(--border); color: var(--text-2); }
  .info .ic { color: var(--muted); }
  .tip { background: var(--accent-softer); border-color: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--text-2); }
  .tip .ic, .tip strong { color: var(--accent); }
  .warn { background: var(--warn-soft); border-color: color-mix(in srgb, var(--warn) 22%, transparent); color: var(--text-2); }
  .warn .ic, .warn strong { color: var(--warn); }
  .danger { background: var(--danger-soft); border-color: color-mix(in srgb, var(--danger) 25%, transparent); color: var(--text-2); }
  .danger .ic, .danger strong { color: var(--danger); }
  .ok { background: var(--ok-soft); border-color: color-mix(in srgb, var(--ok) 22%, transparent); color: var(--text-2); }
  .ok .ic, .ok strong { color: var(--ok); }
</style>
