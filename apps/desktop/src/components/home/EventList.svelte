<script lang="ts" module>
  export interface EventItem {
    id: string;
    tone: "high" | "medium" | "ok" | "neutral";
    title: string;
    detail: string;
    /** Right-aligned meta, e.g. "4m ago". */
    meta?: string;
    /** Omitted when there's nothing to open (e.g. a port that already closed). */
    open?: () => void;
  }
</script>

<script lang="ts">
  // A dense inbox-style list used by "Needs attention" and "Recent changes".
  import Icon from "../Icon.svelte";

  let { items }: { items: EventItem[] } = $props();
</script>

<ul class="list">
  {#each items as item (item.id)}
    <li>
      {#if item.open}
        <button type="button" class="item tone-{item.tone}" data-home-nav onclick={item.open}>
          <span class="dot" aria-hidden="true"></span>
          <span class="title">{item.title}</span>
          <span class="detail">{item.detail}</span>
          {#if item.meta}<span class="meta">{item.meta}</span>{/if}
          <Icon name="chevron" size={12} />
        </button>
      {:else}
        <div class="item tone-{item.tone}">
          <span class="dot" aria-hidden="true"></span>
          <span class="title">{item.title}</span>
          <span class="detail">{item.detail}</span>
          {#if item.meta}<span class="meta">{item.meta}</span>{/if}
          <span class="spacer" aria-hidden="true"></span>
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
  .item { width: 100%; display: grid; grid-template-columns: 8px auto minmax(0, 1fr) auto 12px; align-items: center; gap: var(--sp-2); padding: 5px var(--sp-3); border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; text-align: left; min-width: 0; }
  button.item { cursor: pointer; }
  button.item:hover { background: var(--row-hover); }
  button.item:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .item :global(svg) { color: var(--faint); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
  .tone-high .dot { background: var(--danger); }
  .tone-medium .dot { background: var(--warn); }
  .tone-ok .dot { background: var(--ok); }
  .title { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 30ch; }
  .detail { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .meta { color: var(--faint); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; font-variant-numeric: tabular-nums; }
  @media (max-width: 560px) { .item { grid-template-columns: 8px minmax(0, 1fr) auto 12px; } .detail { display: none; } }
</style>
