<script lang="ts">
  import Icon from "./Icon.svelte";
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
    <span class="num">{entry.port}{#if pinned}<span class="star" title="Pinned" aria-label="pinned"><Icon name="star" size={11} /></span>{/if}</span>
    <span class="meta"><span class="live" class:udp={entry.protocol === "udp"} class:other={entry.state !== "listen" && entry.protocol === "tcp"}></span>{entry.protocol} · {stateLabel}</span>
  </div>

  <FrameworkIcon {entry} />

  <div class="main">
    <div class="line1">
      <span class="title">{title(entry)}</span>
      {#if fw}<span class="fw">{fw}</span>{/if}
      {#if entry.project?.git_branch}
        <span class="branch" title="git branch"><Icon name="branch" size={11} />{entry.project.git_branch}</span>
      {/if}
    </div>
    <div class="line2">
      <span class="mono">{subtitle(entry)}</span>{#if up}<span class="sep">·</span><span>up {up}</span>{/if}
    </div>
  </div>

  <div class="badges">
    {#if links}<span class="badge tone-violet" title="{links} connected service{links === 1 ? '' : 's'} — see the graph view"><Icon name="graph" size={11} />{links}</span>{/if}
    {#if entry.container}<span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>{/if}
    {#if exposed}
      <span class="badge tone-amber" title="Bound to {entry.addresses.join(', ')}: reachable from your network"><Icon name="globe" size={11} />Exposed</span>
    {/if}
    {#if entry.protected}<span class="badge" title="Protected: portwise won't stop this without an explicit override"><Icon name="lock" size={11} />Protected</span>{/if}
    {#if !entry.is_mine && entry.user}<span class="badge" title="Owned by {entry.user}">{entry.user}</span>{/if}
  </div>

  <div class="actions" class:visible={selected || busy}>
    {#if canOpen(entry)}
      <button class="icon-btn" title="Open http://localhost:{entry.port} (O)" aria-label="Open port {entry.port} in browser" onclick={(e) => { e.stopPropagation(); onopen(); }}>
        <Icon name="external" size={15} />
      </button>
    {/if}
    {#if stoppable}
      <button class="btn sm danger-ghost stop" disabled={busy} aria-label="Stop port {entry.port}" title="Stop (⌫)" onclick={(e) => { e.stopPropagation(); onstop(); }}>
        {#if busy}<span class="spin"><Icon name="refresh" size={12} /></span>Stopping{:else}<Icon name="stop" size={10} />Stop{/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .star { display: inline-flex; margin-left: 4px; color: var(--warn); vertical-align: 2px; }
  .star :global(svg) { fill: currentColor; }
  .row {
    display: grid;
    grid-template-columns: 92px 32px minmax(0, 1fr) auto 112px;
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
  .row.selected::before {
    content: ""; position: absolute; left: 4px; top: 14px; bottom: 14px; width: 3px; border-radius: 3px; background: var(--accent);
    animation: grow var(--dur-2) var(--ease);
  }
  @keyframes grow { from { transform: scaleY(0.3); opacity: 0; } }
  .row.busy { opacity: 0.6; }
  .row.dim .title { color: var(--text-2); font-weight: 550; }

  .port { display: flex; flex-direction: column; gap: 3px; }
  .num { font-family: var(--mono); font-size: 18px; font-weight: 650; letter-spacing: -0.03em; line-height: 1; color: var(--text); font-variant-numeric: tabular-nums; }
  .meta { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-2xs); color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-weight: 600; white-space: nowrap; }
  .live { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .live.udp { background: var(--tone-blue); box-shadow: 0 0 0 3px var(--tone-blue-bg); }
  .live.other { background: var(--faint); box-shadow: none; }

  .main { min-width: 0; }
  .line1 { display: flex; align-items: baseline; gap: var(--sp-2); min-width: 0; }
  .title { font-weight: 650; font-size: var(--fs-md); letter-spacing: -0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 0 1 auto; min-width: 0; }
  .fw { font-size: var(--fs-xs); color: var(--muted); font-weight: 550; white-space: nowrap; }
  .branch { display: inline-flex; align-items: center; gap: 3px; font-size: var(--fs-xs); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; font-family: var(--mono); }
  .line2 { color: var(--muted); font-size: var(--fs-xs); margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .line2 .mono { font-size: 11.5px; }
  .sep { margin: 0 6px; opacity: 0.6; }

  .badges { display: flex; gap: var(--sp-1); justify-content: flex-end; }
  .actions { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-1); opacity: 0; transform: translateX(4px); transition: opacity var(--dur-2) var(--ease), transform var(--dur-2) var(--ease); }
  .row:hover .actions, .row:focus-within .actions, .actions.visible { opacity: 1; transform: none; }
  .stop { min-width: 64px; }

  @media (max-width: 1100px) { .badges .badge:not(.tone-amber):not(.tone-blue) { display: none; } }
  @media (max-width: 760px) {
    .row { grid-template-columns: 74px 28px minmax(0, 1fr) auto; }
    .badges { display: none; }
    .num { font-size: 16px; }
  }
</style>
