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
  // Home's summary header: the same tiles as Security and the list's "At a glance"; each opens
  // the view behind it.
  import StatStrip from "../ui/StatStrip.svelte";
  let { stats }: { stats: SummaryStat[] } = $props();
  const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
  const tiles = $derived(stats.map((s) => ({ id: s.id, label: cap(s.label), value: s.value, of: s.of, tone: s.tone, go: s.go, aria: `${cap(s.label)} ${s.value}${s.of != null ? `/${s.of}` : ""}` })));
</script>

<div class="home-stats"><StatStrip stats={tiles} label="Summary" compact /></div>

<style>
  .home-stats { flex: 1 1 520px; min-width: 0; }
</style>
