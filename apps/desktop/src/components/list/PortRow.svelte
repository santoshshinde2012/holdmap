<script lang="ts">
  // One row of the port list. Fixed column grid (shared by every row through the list's
  // container queries) so ports, names, badges and usage line up down the whole list:
  //   status · port · tile + name · meta · badges · usage · age      [actions overlay on hover]
  import Icon from "../Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import StatusDot from "./StatusDot.svelte";
  import RowBadges from "./RowBadges.svelte";
  import Sparkline from "./Sparkline.svelte";
  import { tooltip } from "../../lib/tooltip";
  import type { PortEntry } from "../../lib/types";
  import { canOpen, title } from "../../lib/format";
  import { entryMemory, memLabel, memoryHint, rowBadges, rowFramework, rowLabel, rowMeta, rowStatus, type Density } from "../../lib/rows";

  let {
    entry,
    selected,
    busy,
    onselect,
    onstop,
    onopen,
    onpin,
    pinned = false,
    links = 0,
    usage = [],
    density = "comfortable",
    posinset,
    setsize,
  }: {
    entry: PortEntry;
    pinned?: boolean;
    /** Number of local services connected to/from this one (from the topology). */
    links?: number;
    /** Recent CPU samples (oldest first) for the sparkline. */
    usage?: number[];
    density?: Density;
    posinset?: number;
    setsize?: number;
    selected: boolean;
    busy: boolean;
    onselect: () => void;
    onstop: () => void;
    onopen: () => void;
    onpin?: () => void;
  } = $props();

  const status = $derived(rowStatus(entry, busy));
  const meta = $derived(rowMeta(entry));
  const fw = $derived(rowFramework(entry));
  const badges = $derived(rowBadges(entry, links));
  const stoppable = $derived(!!(entry.process || entry.container) && !entry.protected);
  const mem = $derived(memLabel(entryMemory(entry)));
  const memTip = $derived(memoryHint(entry));
  const cpu = $derived(entry.process?.cpu_percent);
  const usageTip = $derived([cpu !== undefined ? `CPU ${cpu.toFixed(1)}%` : null, memTip ? `Memory ${memTip}` : mem ? `Memory ${mem}` : null].filter(Boolean).join(" · "));
  const tile = $derived(density === "compact" ? 18 : 22);
  const stop = (e: MouseEvent, fn?: () => void) => { e.stopPropagation(); fn?.(); };
</script>

<div
  class="row {density}"
  class:selected
  class:busy
  class:dim={!entry.process && !entry.container}
  role="option"
  id={"row-" + entry.id}
  aria-selected={selected}
  aria-posinset={posinset}
  aria-setsize={setsize}
  aria-label={rowLabel(entry, { pinned, badges })}
  tabindex="-1"
  onclick={onselect}
  ondblclick={() => canOpen(entry) && onopen()}
  onkeydown={() => {}}
>
  <StatusDot status={status.status} label={status.label} />

  <span class="port">{entry.port}</span>

  <span class="name">
    <FrameworkIcon {entry} size={tile} />
    <span class="title" use:tooltip={{ text: title(entry), onlyIfTruncated: true }}>{title(entry)}</span>
    {#if fw}<span class="fw">{fw}</span>{/if}
    {#if pinned}<span class="star" use:tooltip={"Pinned"}><Icon name="star" size={11} /></span>{/if}
  </span>

  <span class="meta">
    <span class="owner">{meta.owner}</span>
    {#if meta.pid}<span class="pid" use:tooltip={"Process ID"}>{meta.pid}</span>{/if}
    {#if meta.branch}<span class="branch" use:tooltip={`Git branch ${meta.branch}`}><Icon name="branch" size={11} /><span class="bt">{meta.branch}</span></span>{/if}
  </span>

  <span class="col-badges"><RowBadges {badges} /></span>

  <span class="usage" use:tooltip={usageTip || null} aria-label={usageTip || undefined}>
    {#if mem || cpu !== undefined}
      <Sparkline values={usage} />
      <span class="mem">{mem ?? "—"}</span>
    {/if}
  </span>

  <span class="age" use:tooltip={meta.uptime ? `Up ${meta.uptime}` : null}>{meta.uptime ?? ""}</span>

  <span class="actions" class:visible={selected || busy}>
    {#if canOpen(entry)}
      <IconButton icon="external" size="sm" label="Open in browser" kbd="O" tabindex={-1} onclick={(e) => stop(e, onopen)} />
    {/if}
    {#if onpin}
      <IconButton icon="star" size="sm" label={pinned ? "Unpin" : "Pin"} kbd="P" pressed={pinned} class="pin" tabindex={-1} onclick={(e) => stop(e, onpin)} />
    {/if}
    {#if stoppable}
      <IconButton icon="stop" size="sm" label={busy ? "Stopping…" : `Stop port ${entry.port}`} kbd="⌫" class="danger" disabled={busy} tabindex={-1} onclick={(e) => stop(e, onstop)} />
    {/if}
  </span>
</div>

<style>
  .row {
    --row-h: 44px;
    --row-tint: transparent;
    position: relative;
    display: grid;
    grid-template-columns: 8px 48px minmax(160px, 1.25fr) minmax(0, 1fr) 160px 84px 52px;
    align-items: center;
    column-gap: var(--sp-3);
    height: var(--row-h);
    padding: 0 var(--sp-3) 0 var(--sp-3);
    margin: 0 var(--sp-2);
    border-radius: var(--r-md);
    background: var(--row-tint);
    cursor: default;
    transition: background var(--dur-1) var(--ease), opacity var(--dur-3) var(--ease);
  }
  .row.compact { --row-h: 36px; }
  /* Hairline between rows, inset to the content; hidden next to tinted rows so they read as one shape. */
  .row::after { content: ""; position: absolute; left: var(--sp-3); right: var(--sp-3); top: -0.5px; height: 1px; background: var(--border); pointer-events: none; }
  .row:first-child::after,
  .row:hover::after, .row.selected::after, :global(.row:hover) + .row::after, :global(.row.selected) + .row::after { opacity: 0; }

  .row:hover { --row-tint: var(--row-hover); }
  .row.selected { --row-tint: var(--row-selected); }
  .row.selected::before {
    content: ""; position: absolute; left: 0; top: 8px; bottom: 8px; width: 2.5px; border-radius: 0 2px 2px 0; background: var(--accent);
    animation: bar var(--dur-2) var(--ease);
  }
  .row.compact.selected::before { top: 6px; bottom: 6px; }
  @keyframes bar { from { transform: scaleY(0.4); opacity: 0; } }
  :global(.list:focus-visible) .row.selected { box-shadow: inset 0 0 0 2px var(--ring); }
  .row.busy { opacity: 0.65; }

  .port { font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  .compact .port { font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); }

  .name { display: flex; align-items: center; gap: var(--sp-2); min-width: 0; }
  .name :global(.tile) { border-radius: 6px; }
  .compact .name :global(.tile) { border-radius: 5px; }
  .name :global(.corner) { border-color: var(--bg); }
  .title { font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); font-weight: var(--fw-medium); color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 0 1 auto; }
  .row.dim .title { color: var(--text-2); }
  .fw { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 0 10 auto; min-width: 0; }
  .star { display: inline-flex; flex: none; color: var(--warn); }
  .star :global(svg) { fill: currentColor; }

  .meta { display: flex; align-items: center; gap: var(--sp-3); min-width: 0; overflow: hidden; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); white-space: nowrap; }
  .owner { overflow: hidden; text-overflow: ellipsis; flex: 0 0 auto; max-width: 60%; }
  .pid, .branch { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--faint); font-variant-numeric: tabular-nums; flex: none; }
  .branch { display: inline-flex; align-items: center; gap: 3px; min-width: 0; flex: 0 1 auto; overflow: hidden; }
  .bt { overflow: hidden; text-overflow: ellipsis; }

  .col-badges { min-width: 0; display: flex; justify-content: flex-end; }
  .usage { display: flex; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
  .mem, .age { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; text-align: right; }
  .age { color: var(--faint); overflow: hidden; }

  /* Actions float over the usage/age columns: no reserved width, no layout shift. */
  .actions {
    position: absolute; right: var(--sp-2); top: 0; bottom: 0; display: flex; align-items: center; gap: 2px; padding-left: 20px;
    background: linear-gradient(90deg, transparent, var(--row-tint) 16px), linear-gradient(90deg, transparent, var(--bg) 16px);
    border-radius: 0 var(--r-md) var(--r-md) 0;
    opacity: 0; pointer-events: none; transition: opacity var(--dur-1) var(--ease);
  }
  .row:hover .actions, .actions.visible { opacity: 1; pointer-events: auto; }
  /* The trailing data columns fade out under the actions instead of showing through them. */
  .usage, .age { transition: opacity var(--dur-1) var(--ease); }
  .row:hover .usage, .row:hover .age, .row.selected .usage, .row.selected .age, .row.busy .usage, .row.busy .age { opacity: 0; }
  .actions :global(.ib.danger:hover:not(:disabled)) { color: var(--danger); background: var(--danger-soft); }
  .actions :global(.ib.pin.on) { color: var(--warn); }
  .actions :global(.ib.pin.on svg) { fill: currentColor; }

  /* Columns drop as the list narrows (the list is the container, so this tracks the pane, not the window). */
  @container portlist (max-width: 900px) { .row { grid-template-columns: 8px 48px minmax(150px, 1.2fr) minmax(0, 1fr) 160px 84px; } .age { display: none; } }
  @container portlist (max-width: 740px) {
    .row { grid-template-columns: 8px 48px minmax(140px, 1.3fr) minmax(0, 1fr) 160px; }
    .usage { display: none; }
    .col-badges { transition: opacity var(--dur-1) var(--ease); }
    .row:hover .col-badges, .row.selected .col-badges { opacity: 0; }
  }
  @container portlist (max-width: 600px) {
    .row { grid-template-columns: 8px 44px minmax(0, 1fr) auto; }
    .meta { display: none; }
    .col-badges :global(.chip:not(:first-child)) { display: none; }
  }
  @container portlist (max-width: 420px) { .row { grid-template-columns: 8px 44px minmax(0, 1fr); } .col-badges, .fw { display: none; } }
  @media (prefers-reduced-motion: reduce) { .row.selected::before { animation: none; } }
</style>
