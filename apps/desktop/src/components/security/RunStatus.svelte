<script lang="ts">
  // Progress for a rescan: the analyzer running now, a bar, and a dismiss when it's over.
  import Icon from "../Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import type { RunProgress } from "../../lib/security-view";
  let { progress, onclose }: { progress: RunProgress; onclose?: () => void } = $props();
  const now = $derived(progress.steps.find((s) => s.phase === "running"));
  const failed = $derived(progress.steps.filter((s) => s.phase === "failed").map((s) => s.name));
  const title = $derived(now ? `Scanning ${now.name} · ${progress.done + 1} of ${progress.total}` : failed.length ? `Failed: ${failed.join(", ")}` : `Scanned ${progress.steps.map((s) => s.name).join(", ")}`);
</script>

{#if progress.total}
  <div class="run ph-{progress.phase}" role="status" aria-live="polite" aria-label="Scan status">
    <div class="top">
      <span class="t">{#if progress.phase === "running"}<span class="spin" aria-hidden="true"></span>{:else}<Icon name={progress.phase === "failed" ? "alert" : "check"} size={13} />{/if}{title}</span>
      {#if progress.phase !== "running" && onclose}<IconButton icon="x" label="Dismiss" size="xs" onclick={onclose} />{/if}
    </div>
    <span class="bar" aria-hidden="true"><i style="width:{(progress.done / progress.total) * 100}%"></i></span>
  </div>
{/if}

<style>
  .run { display: grid; gap: 8px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .t { display: inline-flex; align-items: center; gap: 8px; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .ph-done .t { color: var(--ok); } .ph-failed .t { color: var(--danger); }
  .bar { height: 3px; border-radius: 2px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); transition: width var(--dur-2, 200ms); }
  .spin { width: 11px; height: 11px; border: 2px solid var(--accent-soft); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
