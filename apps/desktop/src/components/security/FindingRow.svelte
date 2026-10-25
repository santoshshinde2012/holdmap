<script lang="ts">
  // One finding: a compact row that expands inline to evidence, location, fix and actions.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import FindingPlace from "./FindingPlace.svelte";
  import type { FindingState } from "../../lib/analyzers";
  import type { FindingItem } from "../../lib/security-view";
  let { item, open = false, busy = false, ontoggle, onreview, onrescan }: {
    item: FindingItem; open?: boolean; busy?: boolean;
    ontoggle: () => void; onreview: (state: FindingState) => void; onrescan?: () => void;
  } = $props();
  const f = $derived(item.finding);
  const id = $derived(`finding-${f.fingerprint}`);
</script>

<li class="f sev-{f.severity}" class:open class:closed={f.state !== "open"}>
  <button type="button" class="head" aria-expanded={open} aria-controls={id} onclick={ontoggle}>
    <Icon name={open ? "chevron-down" : "chevron"} size={12} />
    <span class="sev-pill {f.severity}">{f.severity}</span>
    <span class="title">{f.title}</span>
    <span class="meta"><span class="skill">{item.subject?.name ?? f.subject}</span>{#if item.agents.length}<span class="ag">{item.agents.join(", ")}</span>{/if}{#if item.where}<code>{item.where}</code>{/if}</span>
    <span class="by">{item.analyzerName}</span>
    {#if f.state !== "open"}<span class="st">{f.state}</span>{/if}
  </button>
  {#if open}
    <div class="body" {id}>
      {#if f.fix}<div class="fix"><Icon name="sparkles" size={12} /><span><b>Fix</b> {f.fix}</span></div>{/if}
      <FindingPlace {item} />
      <dl>
        <div><dt>Location</dt><dd>{#if item.where}<code>{item.where}</code>{:else}—{/if}</dd></div>
        <div><dt>Analyzer</dt><dd>{item.analyzerName}</dd></div>
        {#each item.evidence as e (e.label)}<div><dt>{e.label}</dt><dd>{e.value}</dd></div>{/each}
      </dl>
      <div class="acts">
        {#if f.state === "open"}
          <Button size="xs" icon="check" loading={busy} onclick={() => onreview("reviewed")}>Mark reviewed</Button>
          <Button size="xs" variant="ghost" icon="clock" disabled={busy} onclick={() => onreview("snoozed")}>Snooze 7 days</Button>
        {:else}
          <Button size="xs" variant="ghost" icon="undo" loading={busy} onclick={() => onreview("open")}>Reopen</Button>
        {/if}
        {#if onrescan}<Button size="xs" variant="ghost" icon="refresh" disabled={busy} onclick={onrescan} aria-label="Rescan with {item.analyzerName}">Rescan</Button>{/if}
      </div>
    </div>
  {/if}
</li>

<style>
  .f { border-top: 1px solid var(--border); }
  .f:first-child { border-top: 0; }
  .head { display: grid; grid-template-columns: 12px auto minmax(0, 1.4fr) minmax(0, 1fr) auto auto; align-items: center; gap: 10px; width: 100%; padding: 9px 14px; border: 0; background: none; color: var(--text); text-align: left; font: inherit; cursor: pointer; }
  .head:hover { background: var(--row-hover); }
  .head:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .open .head { background: var(--surface-2); }
  .title { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: flex; gap: 8px; min-width: 0; color: var(--muted); font-size: var(--fs-caption); overflow: hidden; white-space: nowrap; }
  .meta > * { overflow: hidden; text-overflow: ellipsis; }
  .by, .st { color: var(--muted); font-size: var(--fs-caption); white-space: nowrap; }
  .st { text-transform: capitalize; }
  .closed .title { color: var(--text-2); }
  code { font-family: var(--mono); font-size: var(--fs-mono-sm); }
  .body { display: grid; gap: 12px; padding: 4px 14px 14px 36px; background: var(--surface-2); }
  .fix { display: flex; gap: 8px; align-items: flex-start; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface); border: 1px solid var(--border); font-size: var(--fs-body-sm); color: var(--text-2); }
  .fix b { color: var(--text); font-weight: var(--fw-semibold); margin-right: 4px; }
  dl { margin: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 8px 16px; }
  dt { color: var(--muted); font-size: var(--fs-caption); }
  dd { margin: 0; font-size: var(--fs-body-sm); color: var(--text); overflow-wrap: anywhere; }
  .acts { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
