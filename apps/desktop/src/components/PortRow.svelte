<script lang="ts">
  import Icon from "./Icon.svelte";
  import IconButton from "./ui/IconButton.svelte";
  import Button from "./ui/Button.svelte";
  import { tooltip } from "../lib/tooltip";
  import FrameworkIcon from "./FrameworkIcon.svelte";
  import type { PortEntry } from "../lib/types";
  import { canOpen, subtitle, title, uptime } from "../lib/format";

  let {
    entry,
    selected,
    busy,
    onselect,
    onstop,
    onopen,
    pinned = false,
    links = 0,
  }: {
    entry: PortEntry;
    pinned?: boolean;
    /** Number of local services connected to/from this one (from the topology). */
    links?: number;
    selected: boolean;
    busy: boolean;
    onselect: () => void;
    onstop: () => void;
    onopen: () => void;
  } = $props();

  const up = $derived(uptime(entry));
  const exposed = $derived(entry.exposure === "all_interfaces");
  const stoppable = $derived(!!(entry.process || entry.container) && !entry.protected);
  const fw = $derived(entry.framework && entry.framework.name !== title(entry) ? entry.framework.name : null);
  const stateLabel = $derived(entry.protocol === "udp" ? "bound" : entry.state === "listen" ? "listening" : entry.state.replace(/_/g, " "));
</script>

<div
  class="row"
  class:selected
  class:busy
  class:dim={!entry.process && !entry.container}
  role="option"
  id={"row-" + entry.id}
  aria-selected={selected}
  aria-label="Port {entry.port} {entry.protocol}, {title(entry)}{fw ? `, ${fw}` : ''}{exposed ? ', exposed to network' : ''}"
  tabindex="-1"
  onclick={onselect}
  ondblclick={() => canOpen(entry) && onopen()}
  onkeydown={() => {}}
>
  <div class="port">
    <span class="num">{entry.port}{#if pinned}<span class="star" use:tooltip={"Pinned"} aria-label="pinned"><Icon name="star" size={11} /></span>{/if}</span>
    <span class="meta"><span class="live" class:udp={entry.protocol === "udp"} class:other={entry.state !== "listen" && entry.protocol === "tcp"}></span>{entry.protocol} · {stateLabel}</span>
  </div>

  <FrameworkIcon {entry} />

  <div class="main">
    <div class="line1">
      <span class="title">{title(entry)}</span>
      {#if fw}<span class="fw">{fw}</span>{/if}
      {#if entry.project?.git_branch}
        <span class="branch" use:tooltip={"Git branch"}><Icon name="branch" size={11} />{entry.project.git_branch}</span>
      {/if}
    </div>
    <div class="line2">
      <span class="mono">{subtitle(entry)}</span>{#if up}<span class="sep">·</span><span>up {up}</span>{/if}
    </div>
  </div>

  <div class="badges">
    {#if links}<span class="badge tone-violet" use:tooltip={`${links} connected service${links === 1 ? "" : "s"} — see the graph view`}><Icon name="graph" size={11} />{links}</span>{/if}
    {#if entry.container}<span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>{/if}
    {#if exposed}
      <span class="badge tone-amber" use:tooltip={`Bound to ${entry.addresses.join(", ")}: reachable from your network`}><Icon name="globe" size={11} />Exposed</span>
    {/if}
    {#if entry.protected}<span class="badge" use:tooltip={"Protected: portwise won't stop this without an explicit override"}><Icon name="lock" size={11} />Protected</span>{/if}
    {#if !entry.is_mine && entry.user}<span class="badge" use:tooltip={`Owned by ${entry.user}`}>{entry.user}</span>{/if}
  </div>

  <div class="actions" class:visible={selected || busy}>
    {#if canOpen(entry)}
      <IconButton icon="external" size="sm" label="Open in browser" kbd="O" onclick={(e) => { e.stopPropagation(); onopen(); }} tabindex={-1} />
    {/if}
    {#if stoppable}
      <Button size="xs" variant="danger-outline" icon="stop" loading={busy} loadingText="Stopping" aria-label="Stop port {entry.port}" tip={{ text: "Stop", kbd: "⌫" }} tabindex={-1} onclick={(e) => { e.stopPropagation(); onstop(); }}>Stop</Button>
    {/if}
  </div>
</div>

<style>
  .star { display: inline-flex; margin-left: 4px; color: var(--warn); vertical-align: 2px; }
  .star :global(svg) { fill: currentColor; }
  .row {
    display: grid;
    grid-template-columns: 104px 32px minmax(0, 1fr) auto 112px;
    align-items: center;
    column-gap: var(--sp-3);
    min-height: 58px;
    padding: var(--sp-2) var(--sp-3) var(--sp-2) var(--sp-4);
    margin: 1px var(--sp-2);
    border-radius: var(--r-md);
    cursor: default;
    position: relative;
    transition: background var(--dur-1) var(--ease), opacity var(--dur-3);
  }
  .row:hover { background: var(--row-hover); }
  .row.selected { background: var(--row-selected); }
  :global(.list:focus-visible) .row.selected { box-shadow: inset 0 0 0 1.5px var(--ring); }
  .row.selected::before {
    content: ""; position: absolute; left: 4px; top: 14px; bottom: 14px; width: 3px; border-radius: 3px; background: var(--accent);
    animation: grow var(--dur-2) var(--ease);
  }
  @keyframes grow { from { transform: scaleY(0.3); opacity: 0; } }
  .row.busy { opacity: 0.6; }
  .row.dim .title { color: var(--text-2); font-weight: var(--fw-medium); }

  .port { display: flex; flex-direction: column; gap: 1px; }
  .num { font-size: var(--fs-title); line-height: var(--lh-title); letter-spacing: var(--ls-title); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  .meta { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-label); line-height: var(--lh-label); color: var(--muted); text-transform: uppercase; letter-spacing: var(--ls-label); font-weight: var(--fw-medium); white-space: nowrap; }
  .live { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .live.udp { background: var(--tone-blue); box-shadow: 0 0 0 3px var(--tone-blue-bg); }
  .live.other { background: var(--faint); box-shadow: none; }

  .main { min-width: 0; }
  .line1 { display: flex; align-items: baseline; gap: var(--sp-2); min-width: 0; }
  .title { font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); font-weight: var(--fw-semibold); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 0 1 auto; min-width: 0; }
  .fw { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); white-space: nowrap; }
  .branch { display: inline-flex; align-items: center; gap: 3px; font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; font-family: var(--mono); }
  .line2 { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .line2 .mono { font-size: var(--fs-mono-sm); line-height: var(--lh-body-sm); }
  .sep { margin: 0 6px; opacity: 0.6; }

  .badges { display: flex; gap: var(--sp-1); justify-content: flex-end; }
  .actions { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-1); opacity: 0; transform: translateX(4px); transition: opacity var(--dur-2) var(--ease), transform var(--dur-2) var(--ease); }
  .row:hover .actions, .row:focus-within .actions, .actions.visible { opacity: 1; transform: none; }

  @media (max-width: 1100px) { .badges .badge:not(.tone-amber):not(.tone-blue) { display: none; } }
  @media (max-width: 760px) {
    .row { grid-template-columns: 74px 28px minmax(0, 1fr) auto; }
    .badges { display: none; }
    .num { font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); }
  }
</style>
