<script lang="ts">
  // One finding: a compact row that expands inline to evidence, location, fix and actions.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import FindingPlace from "./FindingPlace.svelte";
  import type { FindingState } from "../../lib/analyzers";
  import type { SnoozeFor } from "../../lib/api";
  import { dateBounds, SNOOZE_PRESETS, untilFromDate, untilText, MAX_SNOOZE_DAYS } from "../../lib/snooze";
  import type { FindingItem } from "../../lib/security-view";
  let { item, open = false, busy = false, ontoggle, onreview, onrescan }: {
    item: FindingItem; open?: boolean; busy?: boolean;
    ontoggle: () => void; onreview: (state: FindingState, reason?: string, snooze?: SnoozeFor) => void; onrescan?: () => void;
  } = $props();
  let reason = $state("");
  /** Snooze length: a preset's days, or "date" for a picked day (tomorrow to 90 days ahead). */
  let length = $state("7");
  let day = $state("");
  const bounds = $derived(dateBounds(Date.now()));
  const pickedUntil = $derived(length === "date" ? untilFromDate(day, Date.now()) : null);
  const snoozeFor = $derived<SnoozeFor | null>(length === "date" ? (pickedUntil ? { untilMs: pickedUntil } : null) : { days: Number(length) });
  const f = $derived(item.finding);
  const id = $derived(`finding-${f.fingerprint}`);
</script>

<li class="f sev-{f.severity}" class:open class:closed={f.state !== "open"}>
  <button type="button" class="head" aria-expanded={open} aria-controls={id} onclick={ontoggle}>
    <Icon name={open ? "chevron-down" : "chevron"} size={12} />
    <span class="sev-pill {f.severity}">{f.severity}</span>
    <span class="title">{#if item.isNew}<span class="new-pill">New</span> {/if}{f.title}</span>
    <span class="meta"><span class="skill">{item.subject?.name ?? f.subject}</span>{#if item.agents.length}<span class="ag">{item.agents.join(", ")}</span>{/if}{#if item.where}<code>{item.where}</code>{/if}</span>
    <span class="by">{item.analyzerName}</span>
    {#if f.state !== "open"}<span class="st">{f.state === "snoozed" && f.until_ms ? `snoozed ${untilText(f.until_ms)}` : f.state}</span>{/if}
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
      {#if f.state !== "open" && f.note}<p class="note"><b>Reason</b> {f.note}</p>{/if}
      <div class="acts">
        {#if f.state === "open"}
          <input class="reason" type="text" maxlength="200" placeholder="Reason (optional)" aria-label="Reason for {f.title}" bind:value={reason} />
          <Button size="xs" icon="check" loading={busy} onclick={() => onreview("reviewed", reason)}>Mark reviewed</Button>
          <span class="snooze">
            <select aria-label="Snooze length for {f.title}" bind:value={length}>
              {#each SNOOZE_PRESETS as p (p.days)}<option value={String(p.days)}>{p.label}</option>{/each}
              <option value="date">Pick a date…</option>
            </select>
            {#if length === "date"}<input type="date" min={bounds.min} max={bounds.max} bind:value={day} aria-label="Snooze until (up to {MAX_SNOOZE_DAYS} days)" aria-invalid={day !== "" && !pickedUntil} />{/if}
            <Button size="xs" variant="ghost" icon="clock" disabled={busy || !snoozeFor} onclick={() => snoozeFor && onreview("snoozed", reason, snoozeFor)}>Snooze</Button>
          </span>
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
  .acts { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .reason { flex: 1 1 200px; max-width: 360px; height: 24px; padding: 0 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-caption); }
  .snooze { display: inline-flex; align-items: center; gap: 4px; flex-wrap: wrap; }
  .snooze select, .snooze input { height: 24px; padding: 0 6px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-caption); }
  .snooze select:focus-visible, .snooze input:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .reason:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); }
  .note b { color: var(--text); font-weight: var(--fw-semibold); margin-right: 4px; }
</style>
