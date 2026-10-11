<script lang="ts">
  // Header of the details pane: framework tile, port hero, name, status badges and quick actions.
  import Icon from "../Icon.svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import type { PortEntry } from "../../lib/types";
  import { canOpen, title, url } from "../../lib/format";
  import { tooltip } from "../../lib/tooltip";

  let { entry, pinned, onpin, oncopy, onclose }: { entry: PortEntry; pinned: boolean; onpin?: () => void; oncopy: (text: string, what: string) => void; onclose?: () => void } = $props();
  const name = $derived(title(entry));
  const fw = $derived(entry.framework && entry.framework.name !== name ? entry.framework.name : null);
  const status = $derived(entry.state === "listen" ? "Listening" : entry.protocol === "udp" ? "Bound" : entry.state.replace(/_/g, " "));
</script>

<header class="head">
  <div class="top">
    <div class="hero"><span class="port selectable">:{entry.port}</span><span class="proto">{entry.protocol}</span></div>
    <div class="tools">
      {#if onpin}<IconButton icon="star" label={pinned ? `Unpin :${entry.port}` : `Pin :${entry.port}`} kbd="P" pressed={pinned} class="pin" size="sm" onclick={onpin} />{/if}
      {#if canOpen(entry)}<IconButton icon="link" label="Copy URL" kbd="C" size="sm" onclick={() => oncopy(url(entry), "URL")} />{/if}
      {#if onclose}<IconButton icon="x" label="Close details" kbd="Esc" size="sm" onclick={onclose} />{/if}
    </div>
  </div>
  <div class="id">
    <FrameworkIcon {entry} size={32} />
    <div class="who">
      <h2 use:tooltip={{ text: name, onlyIfTruncated: true }}>{name}</h2>
      <div class="meta"><span class="status" class:live={entry.state === "listen" || entry.protocol === "udp"}><span class="dot" aria-hidden="true"></span>{status}</span>{#if fw}<span class="fw">{fw}</span>{/if}</div>
    </div>
  </div>
  <div class="badges">
    {#if entry.project?.git_branch}<span class="badge" use:tooltip={"Git branch"}><Icon name="branch" size={11} />{entry.project.git_branch}</span>{/if}
    {#if entry.exposure === "all_interfaces"}<span class="badge tone-amber"><Icon name="globe" size={11} />Network-exposed</span>{:else if entry.exposure === "specific"}<span class="badge tone-amber"><Icon name="globe" size={11} />Specific interface</span>{:else if entry.exposure === "loopback"}<span class="badge"><Icon name="lock" size={11} />Local only</span>{/if}
    {#if entry.container}<span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>{/if}
    {#if entry.tunnel}<span class="badge tone-violet"><Icon name="link" size={11} />{entry.tunnel.kind}</span>{/if}
    {#if entry.protected}<span class="badge"><Icon name="shield" size={11} />Protected</span>{/if}
    {#if !entry.is_mine && entry.user}<span class="badge"><Icon name="lock" size={11} />{entry.user}</span>{/if}
  </div>
</header>

<style>
  .head { padding: var(--sp-4); display: grid; gap: var(--sp-3); min-width: 0; flex: none; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); min-width: 0; }
  .id { display: flex; gap: var(--sp-2); align-items: center; min-width: 0; }
  .id > :global(.tile) { flex: none; }
  .who { flex: 1; min-width: 0; }
  .hero { display: flex; align-items: center; flex-wrap: wrap; gap: var(--sp-2); min-width: 0; }
  .port { font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); font-variant-numeric: tabular-nums; }
  .proto { font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); border: 1px solid var(--border-strong); border-radius: var(--r-xs); padding: 2px 5px; }
  .status { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); font-weight: var(--fw-medium); text-transform: capitalize; }
  .status .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
  .status.live { color: var(--ok); }
  .status.live .dot { background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  h2 { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; letter-spacing: var(--ls-heading); }
  .meta { display: flex; align-items: center; flex-wrap: wrap; gap: 4px var(--sp-2); min-width: 0; margin-top: 2px; }
  .fw { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  .tools { display: flex; gap: 2px; flex: none; }
  .tools :global(.pin.on) { color: var(--warn); }
  .tools :global(.pin.on svg) { fill: currentColor; }
  .badges { display: flex; flex-wrap: wrap; gap: 6px; min-width: 0; }
  .badges .badge { max-width: 100%; white-space: normal; overflow-wrap: anywhere; }
  .badges .badge :global(svg) { flex: none; }
  .badges:empty { display: none; }
</style>
