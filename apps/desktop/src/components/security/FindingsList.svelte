<script lang="ts">
  // Findings in groups, each a card with a header (label, worst severity, count).
  import FindingRow from "./FindingRow.svelte";
  import type { FindingState } from "../../lib/analyzers";
  import type { FindingGroup, GroupBy } from "../../lib/security-view";
  let { groups, groupBy, busy, onreview, onrescan }: {
    groups: FindingGroup[]; groupBy: GroupBy; busy: string | null;
    onreview: (fingerprint: string, state: FindingState) => void; onrescan?: (analyzer: string) => void;
  } = $props();
  let openId = $state<string | null>(null);
</script>

<div class="groups">
  {#each groups as g (g.key)}
    <section class="group" aria-label="{g.label} findings">
      <header>
        {#if groupBy === "severity" && g.severity}<span class="sev-pill {g.severity}">{g.severity}</span>{:else}<h4>{g.label}</h4>{#if g.severity}<span class="sev-pill {g.severity}">{g.severity}</span>{/if}{/if}
        <span class="n">{g.items.length}</span>
      </header>
      <ul>
        {#each g.items as item (item.finding.fingerprint)}
          <FindingRow {item} open={openId === item.finding.fingerprint} busy={busy === item.finding.fingerprint}
            ontoggle={() => (openId = openId === item.finding.fingerprint ? null : item.finding.fingerprint)}
            onreview={(s) => onreview(item.finding.fingerprint, s)}
            onrescan={onrescan ? () => onrescan(item.analyzer) : undefined} />
        {/each}
      </ul>
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
</style>
