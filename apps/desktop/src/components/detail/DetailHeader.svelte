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
  <div class="id">
    <FrameworkIcon {entry} size={40} />
    <div class="who">
      <div class="hero">
        <span class="port selectable">{entry.port}</span>
        <span class="proto">{entry.protocol}</span>
        <span class="status" class:live={entry.state === "listen" || entry.protocol === "udp"}><span class="dot" aria-hidden="true"></span>{status}</span>
      </div>
      <h2 use:tooltip={{ text: name, onlyIfTruncated: true }}>{name}{#if fw}<span class="fw"><span class="sep" aria-hidden="true">·</span>{fw}</span>{/if}</h2>
    </div>
    <div class="tools">
      {#if onpin}<IconButton icon="star" label={pinned ? `Unpin :${entry.port}` : `Pin :${entry.port}`} kbd="P" pressed={pinned} class="pin" size="sm" onclick={onpin} />{/if}
      {#if canOpen(entry)}<IconButton icon="link" label="Copy URL" kbd="C" size="sm" onclick={() => oncopy(url(entry), "URL")} />{/if}
      {#if onclose}<IconButton icon="x" label="Close details" kbd="Esc" size="sm" onclick={onclose} />{/if}
    </div>
  </div>
  <div class="badges">
    {#if entry.project?.git_branch}<span class="badge" use:tooltip={"Git branch"}><Icon name="branch" size={11} />{entry.project.git_branch}</span>{/if}
    {#if entry.exposure === "all_interfaces"}<span class="badge tone-amber"><Icon name="globe" size={11} />Network-exposed</span>{:else if entry.exposure === "loopback"}<span class="badge"><Icon name="lock" size={11} />Local only</span>{/if}
    {#if entry.container}<span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>{/if}
    {#if entry.tunnel}<span class="badge tone-violet"><Icon name="link" size={11} />{entry.tunnel.kind}</span>{/if}
    {#if entry.protected}<span class="badge"><Icon name="shield" size={11} />Protected</span>{/if}
    {#if !entry.is_mine && entry.user}<span class="badge"><Icon name="lock" size={11} />{entry.user}</span>{/if}
  </div>
</header>

<style>
  .head { padding: var(--sp-4) var(--sp-4) var(--sp-3) var(--sp-5); display: grid; gap: var(--sp-3); }
  .id { display: flex; gap: var(--sp-3); align-items: center; min-width: 0; }
  .who { flex: 1; min-width: 0; }
  .hero { display: flex; align-items: center; gap: 8px; line-height: 1; }
  .port { font-size: var(--fs-display); line-height: var(--lh-display); font-weight: var(--fw-semibold); letter-spacing: var(--ls-display); font-variant-numeric: tabular-nums; }
  .proto { font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); border: 1px solid var(--border-strong); border-radius: var(--r-xs); padding: 2px 5px; }
  .status { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); font-weight: var(--fw-medium); text-transform: capitalize; }
  .status .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
  .status.live { color: var(--ok); }
  .status.live .dot { background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  h2 { margin: 6px 0 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; letter-spacing: var(--ls-heading); }
  .fw { color: var(--muted); font-weight: var(--fw-regular); }
  .sep { margin: 0 0.4em; }
  .tools { display: flex; gap: 2px; align-self: flex-start; }
  .tools :global(.pin.on) { color: var(--warn); }
  .tools :global(.pin.on svg) { fill: currentColor; }
  .badges { display: flex; flex-wrap: wrap; gap: 6px; }
  .badges:empty { display: none; }
</style>
