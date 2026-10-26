<script lang="ts">
  // Findings in groups, each a card with a header (label, worst severity, count).
  import FindingRow from "./FindingRow.svelte";
  import type { FindingState } from "../../lib/analyzers";
  import { PAGE_SIZE, type FindingGroup, type GroupBy } from "../../lib/security-view";
  let { groups, groupBy, busy, onreview, onrescan }: {
    groups: FindingGroup[]; groupBy: GroupBy; busy: string | null;
    onreview: (fingerprint: string, state: FindingState, reason?: string) => void; onrescan?: (analyzer: string) => void;
  } = $props();
  let openId = $state<string | null>(null);
  let limits = $state<Record<string, number>>({});
</script>

<div class="groups">
  {#each groups as g (g.key)}
    {@const limit = limits[g.key] ?? PAGE_SIZE}
    <section class="group" aria-label="{g.label} findings">
      <header>
        {#if groupBy === "severity" && g.severity}<span class="sev-pill {g.severity}">{g.severity}</span>{:else}<h4>{g.label}</h4>{#if g.severity}<span class="sev-pill {g.severity}">{g.severity}</span>{/if}{/if}
        <span class="n">{g.items.length}</span>
      </header>
      <ul>
        {#each g.items.slice(0, limit) as item (item.finding.fingerprint)}
          <FindingRow {item} open={openId === item.finding.fingerprint} busy={busy === item.finding.fingerprint}
            ontoggle={() => (openId = openId === item.finding.fingerprint ? null : item.finding.fingerprint)}
            onreview={(s, reason) => onreview(item.finding.fingerprint, s, reason)}
            onrescan={onrescan ? () => onrescan(item.analyzer) : undefined} />
        {/each}
      </ul>
      {#if g.items.length > limit}
        <p class="more"><span>Showing {limit} of {g.items.length}.</span> <button type="button" onclick={() => (limits[g.key] = limit + PAGE_SIZE)}>Show {Math.min(PAGE_SIZE, g.items.length - limit)} more</button></p>
      {/if}
    </section>
  {/each}
</div>

<style>
  .groups { display: grid; gap: 12px; }
  .group { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  header { display: flex; align-items: center; gap: 8px; padding: 8px 14px; border-bottom: 1px solid var(--border); background: var(--surface-2); }
  h4 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .n { color: var(--muted); font-size: var(--fs-caption); font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; }
  .more { display: flex; gap: 8px; align-items: center; justify-content: center; margin: 0; padding: 8px; border-top: 1px solid var(--border); color: var(--muted); font-size: var(--fs-caption); }
  .more button { border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; padding: 3px 10px; cursor: pointer; }
  .more button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
