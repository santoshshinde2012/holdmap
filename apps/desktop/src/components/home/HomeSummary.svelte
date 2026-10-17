<script lang="ts" module>
  export interface SummaryStat {
    id: string;
    /** null until the first scan; shown as "…", never as zero. */
    value: number | null;
    of?: number | null;
    label: string;
    tone?: "warn" | "ok";
    go: () => void;
  }
</script>

<script lang="ts">
  // Terse figures; each opens the view behind it.
  let { stats }: { stats: SummaryStat[] } = $props();
</script>

<nav class="stats" aria-label="Summary">
  {#each stats as s (s.id)}
    {#if s.value === null}
      <span class="stat" aria-busy="true"><b class="pending">…</b><span>{s.label}</span></span>
    {:else}
      <button type="button" class="stat tone-{s.tone ?? 'plain'}" onclick={s.go} aria-label="{s.value}{s.of != null ? ` of ${s.of}` : ''} {s.label}">
        <b>{s.value}{#if s.of != null}<small>/{s.of}</small>{/if}</b>{" "}<span>{s.label}</span>
      </button>
    {/if}
  {/each}
</nav>

<style>
  .stats { display: flex; flex-wrap: wrap; gap: 2px var(--sp-1); }
  .stat { display: inline-flex; align-items: baseline; gap: 6px; padding: 2px 8px; border: 0; border-radius: var(--r-md); background: none; font: inherit; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); white-space: nowrap; }
  button.stat { cursor: pointer; }
  button.stat:hover { background: var(--row-hover); color: var(--text-2); }
  button.stat:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  b { font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; letter-spacing: var(--ls-title); }
  b small { font-size: var(--fs-body-sm); color: var(--muted); font-weight: var(--fw-medium); }
  b.pending { color: var(--faint); }
  .tone-warn b { color: var(--warn); }
</style>
