<script lang="ts" module>
  export interface SummaryStat {
    id: string;
    /** null until the first scan; shown as "scanning…", never as zero. */
    value: number | null;
    /** e.g. "of 7" for running-of-configured. */
    of?: number | null;
    label: string;
    tone?: "warn" | "ok";
    go: () => void;
  }
</script>

<script lang="ts">
  // The overview header: each figure is a button into the view that explains it.
  let { stats, updated }: { stats: SummaryStat[]; updated: string | null } = $props();
</script>

<header class="summary">
  <div class="title">
    <h1>Overview</h1>
    <span class="updated" aria-live="polite">{updated ? `Updated ${updated}` : "Scanning…"}</span>
  </div>
  <nav class="stats" aria-label="Summary">
    {#each stats as s (s.id)}
      {#if s.value === null}
        <span class="stat pending" aria-busy="true"><span class="v">scanning…</span><span class="l">{s.label}</span></span>
      {:else}
        <button type="button" class="stat tone-{s.tone ?? 'plain'}" onclick={s.go} aria-label="{s.value}{s.of != null ? ` of ${s.of}` : ''} {s.label}">
          <span class="v">{s.value}{#if s.of != null}<span class="of"> / {s.of}</span>{/if}</span>
          <span class="l">{s.label}</span>
        </button>
      {/if}
    {/each}
  </nav>
</header>

<style>
  .summary { display: flex; align-items: end; justify-content: space-between; gap: var(--sp-3) var(--sp-6); flex-wrap: wrap; }
  .title { display: grid; gap: 2px; }
  h1 { margin: 0; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); color: var(--text); }
  .updated { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .stats { display: flex; flex-wrap: wrap; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  .stat { display: grid; gap: 1px; padding: 8px 16px; min-width: 104px; border: 0; border-right: 1px solid var(--border); background: none; font: inherit; color: var(--text); text-align: left; }
  .stat:last-child { border-right: 0; }
  button.stat { cursor: pointer; }
  button.stat:hover { background: var(--row-hover); }
  button.stat:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .v { font-size: var(--fs-display); line-height: var(--lh-display); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; letter-spacing: var(--ls-display); }
  .of { font-size: var(--fs-body-sm); color: var(--muted); font-weight: var(--fw-medium); }
  .l { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  .tone-warn .v { color: var(--warn); }
  .tone-ok .v { color: var(--ok); }
  .pending .v { font-size: var(--fs-body-sm); line-height: var(--lh-display); color: var(--faint); font-weight: var(--fw-regular); }
  @media (max-width: 640px) {
    .stats { width: 100%; display: grid; grid-template-columns: repeat(auto-fit, minmax(96px, 1fr)); }
    .stat { border-right: 0; border-bottom: 1px solid var(--border); min-width: 0; padding: 6px 12px; }
  }
</style>
