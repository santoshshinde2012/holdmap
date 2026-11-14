<script lang="ts">
  // One sortable table of an agent's processes: RAM, CPU and role, or the parent/child tree.
  import { humanBytes } from "../../lib/format";
  import { nextSort, sortProcessRows, type ProcessSortKey, type ProcessTableRow, type SortDirection } from "../../lib/process-table";

  let { rows, label, color, more = 0 }: { rows: ProcessTableRow[]; label: string; color: (role: string) => string; more?: number } = $props();

  let tree = $state(true);
  let sort = $state<{ key: ProcessSortKey; direction: SortDirection }>({ key: "memory", direction: "desc" });
  const shown = $derived(tree ? rows : sortProcessRows(rows, sort.key, sort.direction));
  const max = $derived(Math.max(1, ...rows.map((r) => r.bytes)));
  const columns: [ProcessSortKey, string][] = [["name", "Process"], ["memory", "RAM"], ["cpu", "CPU"]];
  const ariaSort = (key: ProcessSortKey) => tree || sort.key !== key ? "none" : sort.direction === "asc" ? "ascending" : "descending";
  function pick(key: ProcessSortKey) {
    sort = tree ? { key, direction: key === "name" ? "asc" : "desc" } : nextSort(sort, key);
    tree = false;
  }
</script>

<div class="ptable">
  <button type="button" class="tree-toggle" aria-pressed={tree} onclick={() => (tree = !tree)}>Tree</button>
  <table aria-label={label}>
    <thead>
      <tr>
        {#each columns as [key, text] (key)}
          <th scope="col" aria-sort={ariaSort(key)} class:num={key !== "name"}><button type="button" onclick={() => pick(key)}>{text}{#if !tree && sort.key === key}<span aria-hidden="true">{sort.direction === "asc" ? " ↑" : " ↓"}</span>{/if}</button></th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each shown as row (row.pid)}
        <tr class:large={row.dominant} title={row.command || undefined}>
          <th scope="row" style="padding-left: {tree ? row.depth * 10 + 4 : 4}px">
            <span class="pname"><i style="background: {color(row.role)}" aria-hidden="true"></i>{row.name}</span>
            <span class="psub mono">pid {row.pid} · {row.roleLabel}{#if row.dominant}{" "}· Large{/if}</span>
            {#if row.bits}<span class="psub">{row.bits}</span>{/if}
            {#if row.dominant && row.dominantReason}<span class="psub why">{row.dominantReason}</span>{/if}
          </th>
          <td class="num"><b>{humanBytes(row.bytes)}</b><span class="bar" style="width: {Math.max(4, Math.round((row.bytes / max) * 100))}%" aria-hidden="true"></span></td>
          <td class="num">{row.cpu == null ? "unknown" : `${row.cpu.toFixed(row.cpu < 10 ? 1 : 0)}%`}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if more}<p class="more">+{more} more processes</p>{/if}
</div>

<style>
  .ptable { display: grid; gap: 6px; min-width: 0; }
  .tree-toggle { justify-self: end; border: 1px solid var(--border); border-radius: var(--r-full); padding: 1px 10px; background: var(--surface-2); color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .tree-toggle[aria-pressed="true"] { background: var(--surface); color: var(--text); border-color: var(--border-strong, var(--border)); }
  table { width: 100%; border-collapse: collapse; table-layout: fixed; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  thead th { text-align: left; font-weight: var(--fw-medium); color: var(--muted); border-bottom: 1px solid var(--border); }
  thead th:first-child { width: 58%; }
  thead button { border: 0; background: none; padding: 2px 4px; color: inherit; font: inherit; cursor: pointer; border-radius: var(--r-sm); }
  thead button:hover { color: var(--text); }
  .tree-toggle:focus-visible, thead button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  th.num, td.num { text-align: right; }
  tbody tr { border-bottom: 1px solid var(--border); }
  tbody tr:last-child { border-bottom: 0; }
  tbody th { text-align: left; font-weight: var(--fw-regular); padding-top: 4px; padding-bottom: 4px; min-width: 0; }
  td { padding: 4px; vertical-align: top; color: var(--text-2); font-variant-numeric: tabular-nums; position: relative; }
  td b { color: var(--text); font-weight: var(--fw-semibold); }
  .bar { position: absolute; right: 4px; bottom: 1px; height: 2px; border-radius: 1px; background: color-mix(in srgb, var(--agent, var(--accent)) 55%, var(--surface)); opacity: 0.7; }
  .large .bar { background: var(--warn); }
  .pname { display: flex; align-items: center; gap: 5px; color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pname i { width: 6px; height: 6px; border-radius: 50%; flex: none; }
  .psub { display: block; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .psub.why { white-space: normal; color: var(--warn); }
  .more { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
</style>
