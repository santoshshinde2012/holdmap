<script lang="ts">
  import type { SnoozeFor } from "../../lib/api";
  // Findings in groups, each a card with a header (label, worst severity, count).
  import FindingRow from "./FindingRow.svelte";
  import type { FindingState } from "../../lib/analyzers";
  import { PAGE_SIZE, type FindingGroup, type GroupBy } from "../../lib/security-view";
  let { groups, groupBy, busy, onreview, onrescan }: {
    groups: FindingGroup[]; groupBy: GroupBy; busy: string | null;
    onreview: (fingerprint: string, state: FindingState, reason?: string, snooze?: SnoozeFor) => void; onrescan?: (analyzer: string) => void;
  } = $props();
  let openId = $state<string | null>(null);
  let limits = $state<Record<string, number>>({});
</script>

<div class="sheet">
  <div class="cols" aria-hidden="true"><span></span><span>Severity</span><span>Finding</span><span>Skill</span><span>Agent</span><span>Path</span><span>Analyzer</span></div>
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
            onreview={(s, reason, snooze) => onreview(item.finding.fingerprint, s, reason, snooze)}
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
  .sheet { overflow: auto; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .group + .group { border-top: 1px solid var(--border); }
  header { display: flex; align-items: center; gap: 8px; min-height: 36px; padding: 8px 14px; border-bottom: 1px solid var(--border); background: var(--surface-2); box-sizing: border-box; }
  h4 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .n { color: var(--muted); font-size: var(--fs-caption); font-variant-numeric: tabular-nums; }
  .cols { position: sticky; top: 0; z-index: 1; display: grid; grid-template-columns: 16px 72px minmax(160px, 1.5fr) minmax(96px, 0.7fr) minmax(108px, 0.8fr) minmax(140px, 1fr) 104px; column-gap: 12px; align-items: center; min-width: 920px; min-height: 28px; padding: 0 14px; box-sizing: border-box; color: var(--muted); font-size: var(--fs-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; border-bottom: 1px solid var(--border); background: var(--surface-2); }
  ul { list-style: none; margin: 0; padding: 0; }
  .more { display: flex; gap: 8px; align-items: center; justify-content: center; margin: 0; padding: 8px; border-top: 1px solid var(--border); color: var(--muted); font-size: var(--fs-caption); }
  .more button { border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; padding: 3px 10px; cursor: pointer; }
  .more button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
