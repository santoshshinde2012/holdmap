<script lang="ts">
  // Search, agent filter, group-by and "show closed" for the findings list.
  import Icon from "../Icon.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import type { FindingFilter, GroupBy } from "../../lib/security-view";
  let { filter = $bindable(), groupBy = $bindable(), agents, shown }: { filter: FindingFilter; groupBy: GroupBy; agents: { product: string; name: string }[]; shown: number } = $props();
</script>

<div class="bar" role="toolbar" aria-label="Filter findings">
  <label class="search"><Icon name="search" size={13} /><input type="search" placeholder="Search findings" aria-label="Search findings" bind:value={filter.query} /></label>
  <select aria-label="Agent" bind:value={filter.agent}>
    <option value={null}>All agents</option>
    {#each agents as a (a.product)}<option value={a.product}>{a.name}</option>{/each}
  </select>
  <label class="closed"><input type="checkbox" bind:checked={filter.newOnly} />New since last run</label>
  <label class="closed"><input type="checkbox" bind:checked={filter.showClosed} />Show reviewed</label>
  <span class="n" aria-live="polite">{shown} shown</span>
  <SegmentedControl bind:value={groupBy} label="Group findings by" options={[{ value: "severity", label: "Severity" }, { value: "skill", label: "Skill" }, { value: "agent", label: "Agent" }, { value: "folder", label: "Folder" }, { value: "analyzer", label: "Analyzer" }]} />
</div>

<style>
  .bar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .search { flex: 1 1 220px; display: flex; align-items: center; gap: 6px; height: var(--h-sm, 28px); padding: 0 10px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--muted); }
  .search:focus-within { outline: 2px solid var(--ring); outline-offset: 1px; }
  input[type="search"] { flex: 1; min-width: 0; border: 0; outline: 0; background: none; color: var(--text); font: inherit; font-size: var(--fs-body-sm); }
  select { height: var(--h-sm, 28px); padding: 0 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body-sm); }
  select:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .closed { display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); font-size: var(--fs-body-sm); }
  .n { margin: 0 4px 0 auto; color: var(--muted); font-size: var(--fs-caption); font-variant-numeric: tabular-nums; }
</style>
