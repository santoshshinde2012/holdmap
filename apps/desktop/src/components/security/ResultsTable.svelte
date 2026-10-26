<script lang="ts">
  // The results table: one row per finding with severity, rule, message, agent, skill, folder
  // and path:line. Headers sort; a row expands to where it lives and what to do next.
  import Icon from "../Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import FindingPlace from "./FindingPlace.svelte";
  import { redactHome } from "../../lib/report-export";
  import { PAGE_SIZE, type FindingItem, type SortKey, type SortSpec } from "../../lib/security-view";

  let { items, sort = $bindable() }: { items: FindingItem[]; sort: SortSpec } = $props();
  let openId = $state<string | null>(null);
  let limit = $state(PAGE_SIZE);
  const rows = $derived(items.slice(0, limit));
  const COLS: { key: SortKey; label: string }[] = [
    { key: "severity", label: "Severity" }, { key: "rule", label: "Rule" }, { key: "message", label: "Message" },
    { key: "agent", label: "Agent" }, { key: "skill", label: "Skill" }, { key: "folder", label: "Folder" }, { key: "path", label: "Path" },
  ];
  function by(key: SortKey) {
    sort = sort.key === key ? { key, dir: sort.dir === "asc" ? "desc" : "asc" } : { key, dir: key === "severity" ? "desc" : "asc" };
  }
  const ariaSort = (k: SortKey) => (sort.key === k ? (sort.dir === "asc" ? "ascending" : "descending") : "none");
  const copy = (t: string) => navigator.clipboard?.writeText(t);
  const short = (dir: string | null) => (dir ? redactHome(dir).split("/").slice(-3).join("/") : "unknown");
</script>

<div class="wrap" tabindex="-1">
  <table data-results-table>
    <caption class="sr-only">Findings, {items.length} shown. Sorted by {sort.key}, {sort.dir === "asc" ? "ascending" : "descending"}.</caption>
    <thead>
      <tr>
        <th scope="col" class="x"><span class="sr-only">Details</span></th>
        {#each COLS as c (c.key)}
          <th scope="col" aria-sort={ariaSort(c.key)}><button type="button" onclick={() => by(c.key)}>{c.label}{#if sort.key === c.key}<Icon name={sort.dir === "asc" ? "chevron-up" : "chevron-down"} size={11} />{/if}</button></th>
        {/each}
        <th scope="col" class="x"><span class="sr-only">Actions</span></th>
      </tr>
    </thead>
    <tbody>
      {#each rows as i (i.finding.fingerprint)}
        {@const open = openId === i.finding.fingerprint}
        <tr class:open class:closed={i.finding.state !== "open"}>
          <td class="x"><IconButton size="xs" icon={open ? "chevron-down" : "chevron"} label="{open ? 'Hide' : 'Show'} details for {i.finding.title}" pressed={open} onclick={() => (openId = open ? null : i.finding.fingerprint)} /></td>
          <td><span class="sev-pill {i.finding.severity}">{i.finding.severity}</span></td>
          <td><code>{i.finding.rule_id}</code></td>
          <td class="msg">{#if i.isNew}<span class="new-pill">New</span> {/if}{i.finding.title}{#if i.finding.state !== "open"} <small>({i.finding.state})</small>{/if}</td>
          <td>{i.agents.length ? i.agents.join(", ") : "unknown"}</td>
          <td>{i.subject?.name ?? i.finding.subject}</td>
          <td class="dir" title={i.folder ? redactHome(i.folder) : "holdmap couldn't confirm the folder"}>{short(i.folder)}</td>
          <td class="dir">{#if i.where}<code>{i.where}</code>{:else}unknown{/if}</td>
          <td class="x">{#if i.path}<IconButton size="xs" icon="copy" label="Copy path of {i.finding.title}" onclick={() => copy(i.path + (i.finding.location.line ? `:${i.finding.location.line}` : ""))} />{/if}</td>
        </tr>
        {#if open}
          <tr class="detail"><td colspan={COLS.length + 2}><FindingPlace item={i} />{#if i.finding.fix}<p class="fix"><b>Fix</b> {i.finding.fix}</p>{/if}{#if i.finding.note}<p class="fix"><b>Reason</b> {i.finding.note}</p>{/if}</td></tr>
        {/if}
      {/each}
    </tbody>
  </table>
  {#if items.length > rows.length}
    <p class="more"><span>Showing {rows.length} of {items.length}.</span> <button type="button" onclick={() => (limit += PAGE_SIZE)}>Show {Math.min(PAGE_SIZE, items.length - rows.length)} more</button></p>
  {/if}
</div>

<style>
  .wrap { overflow: auto; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body-sm); }
  th { position: sticky; top: 0; background: var(--surface-2); text-align: left; font-weight: var(--fw-semibold); color: var(--text-2); border-bottom: 1px solid var(--border); white-space: nowrap; }
  th button { display: inline-flex; align-items: center; gap: 4px; width: 100%; padding: 8px 10px; border: 0; background: none; color: inherit; font: inherit; cursor: pointer; }
  th button:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  td { padding: 6px 10px; border-top: 1px solid var(--border); vertical-align: top; color: var(--text); }
  tr:first-child td { border-top: 0; }
  td.x, th.x { width: 28px; padding: 4px; }
  .msg { min-width: 200px; }
  .dir { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .closed td { color: var(--text-2); }
  .open td { background: var(--surface-2); }
  .detail td { background: var(--surface-2); padding: 8px 14px 14px 38px; }
  code { font-family: var(--mono); font-size: var(--fs-mono-sm); }
  small { color: var(--muted); }
  .fix { margin: 8px 0 0; color: var(--text-2); }
  .fix b { color: var(--text); margin-right: 4px; }
  .more { display: flex; gap: 8px; align-items: center; justify-content: center; margin: 0; padding: 8px; border-top: 1px solid var(--border); color: var(--muted); font-size: var(--fs-caption); }
  .more button { border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; padding: 3px 10px; cursor: pointer; }
  .more button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
