<script lang="ts" module>
  export interface Stat {
    id: string;
    label: string;
    /** null until known: shown as "…", never as zero. */
    value: number | string | null;
    of?: number | null;
    meta?: string;
    tone?: "plain" | "warn" | "ok";
    /** Draw a coverage bar (value / of). */
    bar?: boolean;
    go?: () => void;
    title?: string;
    /** Accessible name for a clickable tile; defaults to its visible text. */
    aria?: string;
  }
</script>

<script lang="ts">
  // The summary header used on every screen: bordered tiles with a small label, a big figure and
  // an optional meta line or coverage bar. A tile with `go` is a button that opens what it counts.
  import type { Snippet } from "svelte";
  let { stats, label = "Summary", lead, compact = false }: { stats: Stat[]; label?: string; lead?: Snippet; compact?: boolean } = $props();
  const pct = (s: Stat) => (typeof s.value === "number" && s.of ? Math.round((s.value / s.of) * 100) : 0);
</script>

<div class="strip" class:compact class:has-lead={!!lead} role="group" aria-label={label} style="--n: {stats.length}">
  {#if lead}<div class="tile lead">{@render lead()}</div>{/if}
  {#each stats as s (s.id)}
    <svelte:element this={s.go && s.value !== null ? "button" : "div"} type={s.go && s.value !== null ? "button" : undefined}
      class="tile tone-{s.tone ?? 'plain'}" class:go={!!s.go} title={s.title} aria-label={s.go && s.value !== null ? s.aria : undefined} aria-busy={s.value === null ? "true" : undefined}
      onclick={s.go && s.value !== null ? s.go : undefined} role={s.go && s.value !== null ? undefined : "presentation"}>
      <span class="k">{s.label}</span>
      <b class:pending={s.value === null}>{s.value ?? "…"}{#if s.of != null && s.value !== null}<small>/{s.of}</small>{/if}</b>
      {#if s.bar && s.of != null}<span class="bar" aria-hidden="true"><i style="width:{pct(s)}%"></i></span>
      {:else if s.meta}<span class="m">{s.meta}</span>{/if}
    </svelte:element>
  {/each}
</div>

<style>
  /* 1px gaps over a border-coloured backdrop draw the dividers, so tiles can wrap on narrow windows. */
  .strip { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 1px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--border); overflow: hidden; }
  .compact { grid-template-columns: repeat(auto-fit, minmax(104px, 1fr)); }
  .strip.has-lead { grid-template-columns: minmax(0, 2fr) repeat(var(--n), minmax(0, 1fr)); }
  @media (max-width: 720px) { .strip.has-lead { grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); } .lead { grid-column: 1 / -1; } }
  .tile { display: grid; align-content: start; gap: 4px; padding: 12px 16px; min-width: 0; border: 0; background: var(--surface); font: inherit; color: inherit; text-align: left; }
  .compact .tile { padding: 10px 12px; gap: 2px; }
  .go { cursor: pointer; transition: background var(--dur-1) var(--ease); }
  .go:hover { background: linear-gradient(var(--row-hover), var(--row-hover)) var(--surface); }
  .go:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .k { color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  b { font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; color: var(--text); }
  b small { font-size: var(--fs-body-sm); color: var(--muted); font-weight: var(--fw-medium); }
  b.pending { color: var(--faint); }
  .tone-warn b { color: var(--warn); }
  .tone-ok b { color: var(--ok); }
  .m { color: var(--muted); font-size: var(--fs-caption); overflow-wrap: anywhere; }
  .bar { height: 4px; border-radius: 2px; background: var(--surface-3); overflow: hidden; margin-top: 2px; }
  .bar i { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
</style>
