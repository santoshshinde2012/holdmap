<script lang="ts">
  // Compact "Needs you": severity, title and actions; the detail opens on expand.
  // Stop always opens the stop plan for review first.
  import Icon from "../Icon.svelte";
  import type { AttentionItem } from "../../lib/home";

  let { items, lit = null, onopen, onstop, onhover }: {
    items: AttentionItem[];
    /** Items of the active agent; others dim. null shows all. */
    lit?: Set<string> | null;
    onopen: (item: AttentionItem) => void;
    onstop: (item: AttentionItem) => void;
    onhover: (item: AttentionItem | null) => void;
  } = $props();
  let open = $state<Record<string, boolean>>({});
</script>

<ul class="needs">
  {#each items as item (item.id)}
    {@const expanded = !!open[item.id]}
    <li class="sev-{item.severity}" class:dim={lit !== null && !lit.has(item.id)} onmouseenter={() => onhover(item)} onmouseleave={() => onhover(null)}>
      <button type="button" class="title" aria-expanded={expanded} aria-controls="need-{item.id}" title={item.detail}
        onclick={() => (open = { ...open, [item.id]: !expanded })} onfocus={() => onhover(item)} onblur={() => onhover(null)}>
        <span class="dot" aria-hidden="true"></span><span class="t">{item.title}</span>
      </button>
      <span class="acts">
        {#if item.stop}<button type="button" class="act" onclick={() => onstop(item)} aria-label="Review stop plan: {item.title}"><Icon name="stop" size={11} />Stop…</button>{/if}
        <button type="button" class="act icon" onclick={() => onopen(item)} aria-label="Open: {item.title}"><Icon name="chevron" size={12} /></button>
      </span>
      {#if expanded}<p class="detail" id="need-{item.id}">{item.detail}</p>{/if}
    </li>
  {/each}
</ul>

<style>
  .needs { list-style: none; margin: 0; padding: 0; display: grid; }
  li { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: start; gap: 2px var(--sp-2); padding: 7px var(--sp-3); border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: 0; }
  li { transition: opacity 160ms ease; }
  li.dim { opacity: 0.4; }
  li:hover, li:focus-within { background: var(--row-hover); }
  .title { display: grid; grid-template-columns: 8px minmax(0, 1fr); gap: var(--sp-2); align-items: start; border: 0; background: none; padding: 0; font: inherit; color: var(--text); text-align: left; cursor: pointer; }
  .dot { width: 7px; height: 7px; margin-top: 5px; border-radius: 50%; background: var(--warn); }
  .sev-high .dot { background: var(--danger); box-shadow: 0 0 0 3px var(--danger-soft); }
  .t { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .title[aria-expanded="true"] .t { color: var(--text); }
  .acts { display: flex; gap: 4px; }
  .act { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-caption); cursor: pointer; white-space: nowrap; }
  .act.icon { width: 22px; padding: 0; justify-content: center; }
  .act:hover { color: var(--text); border-color: var(--border-strong); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; border-radius: var(--r-sm); }
  .detail { grid-column: 1 / -1; margin: 2px 0 0 15px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
</style>
