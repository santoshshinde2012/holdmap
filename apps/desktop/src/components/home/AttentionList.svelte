<script lang="ts">
  // What needs a look, severity first, with the whole message and a one-click next step.
  // Stopping always opens the stop plan for review; nothing stops from here directly.
  import Icon from "../Icon.svelte";
  import type { AttentionItem } from "../../lib/home";

  let { items, onopen, onstop }: { items: AttentionItem[]; onopen: (item: AttentionItem) => void; onstop: (item: AttentionItem) => void } = $props();
</script>

<ul class="list">
  {#each items as item (item.id)}
    <li class="item sev-{item.severity}">
      <span class="sev">{item.severity === "high" ? "High" : "Medium"}</span>
      <div class="text">
        <span class="title" data-full>{item.title}</span>
        <span class="detail" data-full>{item.detail}</span>
      </div>
      <div class="actions">
        {#if item.stop}<button type="button" class="btn" onclick={() => onstop(item)}><Icon name="stop" size={12} />Review stop plan</button>{/if}
        <button type="button" class="btn primary" data-home-nav onclick={() => onopen(item)} aria-label="Open: {item.title}">Open<Icon name="chevron" size={12} /></button>
      </div>
    </li>
  {/each}
</ul>

<style>
  .list { list-style: none; margin: 0; padding: 0; display: grid; }
  .item { display: grid; grid-template-columns: 64px minmax(0, 1fr) auto; align-items: start; gap: var(--sp-3); padding: 9px var(--sp-3); border-bottom: 1px solid var(--border); }
  .item:last-child { border-bottom: 0; }
  .item:hover, .item:focus-within { background: var(--row-hover); }
  .sev { justify-self: start; margin-top: 1px; padding: 0 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); background: var(--warn-soft); color: var(--warn); }
  .sev-high .sev { background: var(--danger-soft); color: var(--danger); }
  .text { display: grid; gap: 1px; min-width: 0; }
  .title { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); overflow-wrap: anywhere; }
  .detail { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow-wrap: anywhere; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; justify-content: end; }
  .btn { display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-caption); cursor: pointer; white-space: nowrap; }
  .btn:hover { color: var(--text); background: var(--row-hover); }
  .btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .btn.primary { color: var(--accent); }
  @media (max-width: 640px) {
    .item { grid-template-columns: minmax(0, 1fr); gap: 6px; }
    .actions { justify-content: start; }
  }
</style>
