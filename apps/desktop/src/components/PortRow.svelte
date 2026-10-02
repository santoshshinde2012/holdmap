<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { PortEntry } from "../lib/types";
  import { addressLabel, canOpen, subtitle, title, tone, uptime } from "../lib/format";

  let {
    entry,
    selected,
    busy,
    onselect,
    onstop,
    onopen,
  }: {
    entry: PortEntry;
    selected: boolean;
    busy: boolean;
    onselect: () => void;
    onstop: () => void;
    onopen: () => void;
  } = $props();

  const t = $derived(tone(entry));
  const up = $derived(uptime(entry));
</script>

<div
  class="row"
  class:selected
  class:dim={entry.protected || !entry.process}
  role="option"
  id={"row-" + entry.id}
  aria-selected={selected}
  tabindex="-1"
  onclick={onselect}
  onkeydown={() => {}}
>
  <div class="port">
    <span class="num mono">{entry.port}</span>
    <span class="proto">{entry.protocol}</span>
  </div>

  <div class="main">
    <div class="line1">
      <span class="title">{title(entry)}</span>
      {#if entry.framework && entry.framework.name !== title(entry)}
        <span class="badge tone-{t}"><span class="dot"></span>{entry.framework.name}</span>
      {:else if entry.container}
        <span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>
      {/if}
      {#if entry.project?.git_branch}
        <span class="branch"><Icon name="branch" size={11} />{entry.project.git_branch}</span>
      {/if}
      {#if entry.protected}
        <span class="lock" title="Protected: portwise won't stop this without an explicit override"><Icon name="lock" size={12} label="Protected" /></span>
      {/if}
    </div>
    <div class="line2 mono">
      {subtitle(entry)}{#if up}<span class="sep">·</span>up {up}{/if}
    </div>
  </div>

  <div class="addr" class:exposed={entry.exposure === "all_interfaces"} title={entry.addresses.join(", ")}>
    {#if entry.exposure === "all_interfaces"}<Icon name="globe" size={12} />{/if}
    {addressLabel(entry)}
  </div>

  <div class="actions" class:visible={selected}>
    {#if canOpen(entry)}
      <button class="icon-btn" title="Open http://localhost:{entry.port} (O)" aria-label="Open in browser" onclick={(e) => { e.stopPropagation(); onopen(); }}>
        <Icon name="external" size={15} />
      </button>
    {/if}
    {#if entry.process || entry.container}
      <button
        class="btn sm danger-ghost"
        disabled={busy}
        aria-label="Stop port {entry.port}"
        title="Stop (⌫)"
        onclick={(e) => { e.stopPropagation(); onstop(); }}
      >
        {#if busy}<span class="spin"><Icon name="refresh" size={12} /></span>{:else}<Icon name="stop" size={11} />{/if}
        Stop
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 74px minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 14px;
    padding: 9px 14px 9px 12px;
    margin: 0 8px;
    border-radius: var(--radius);
    cursor: default;
    position: relative;
    transition: background 0.12s var(--ease);
  }
  .row:hover { background: var(--row-hover); }
  .row.selected { background: var(--row-selected); }
  .row.selected::before {
    content: ""; position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 3px; background: var(--accent);
  }
  .row.dim .title { color: var(--text-2); }
  .port { display: flex; flex-direction: column; align-items: flex-start; line-height: 1.1; }
  .num { font-size: 15px; font-weight: 650; letter-spacing: -0.01em; color: var(--text); }
  .proto { font-size: 10px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--faint); margin-top: 3px; font-weight: 600; }
  .main { min-width: 0; }
  .line1 { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .title { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .branch { display: inline-flex; align-items: center; gap: 3px; font-size: 11.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lock { color: var(--faint); display: inline-flex; }
  .line2 { color: var(--muted); font-size: 11.5px; margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sep { margin: 0 6px; color: var(--faint); }
  .addr { display: inline-flex; align-items: center; gap: 5px; font-size: 11.5px; color: var(--muted); white-space: nowrap; }
  .addr.exposed { color: var(--warn); font-weight: 500; }
  .actions { display: flex; align-items: center; justify-content: flex-end; gap: 4px; opacity: 0; transition: opacity 0.12s; width: 104px; }
  .row:hover .actions, .actions.visible { opacity: 1; }
  @media (max-width: 980px) { .addr { display: none; } .row { grid-template-columns: 64px minmax(0, 1fr) auto; } }
</style>
