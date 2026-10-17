<script lang="ts" module>
  export interface EventItem {
    id: string;
    tone: "ok" | "neutral" | "warn";
    title: string;
    detail: string;
    meta?: string;
    /** Omitted when there's nothing left to open (e.g. a port that already closed). */
    open?: () => void;
  }
</script>

<script lang="ts">
  // A compact timeline row list; titles and details wrap rather than cut off.
  let { items }: { items: EventItem[] } = $props();
</script>

<ul class="list">
  {#each items as item (item.id)}
    <li class="tone-{item.tone}">
      <span class="dot" aria-hidden="true"></span>
      {#if item.open}
        <button type="button" class="text" data-home-nav onclick={item.open}><span class="title">{item.title}</span><span class="detail">{item.detail}</span></button>
      {:else}
        <span class="text"><span class="title">{item.title}</span><span class="detail">{item.detail}</span></span>
      {/if}
      {#if item.meta}<span class="meta">{item.meta}</span>{/if}
    </li>
  {/each}
</ul>

<style>
  .list { list-style: none; margin: 0; padding: 0; display: grid; }
  li { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; gap: var(--sp-2); align-items: start; padding: 6px var(--sp-3); }
  li:hover { background: var(--row-hover); }
  .dot { width: 7px; height: 7px; margin-top: 6px; border-radius: 50%; background: var(--faint); }
  .tone-ok .dot { background: var(--ok); }
  .tone-warn .dot { background: var(--warn); }
  .text { display: grid; min-width: 0; border: 0; padding: 0; background: none; font: inherit; color: inherit; text-align: left; }
  button.text { cursor: pointer; border-radius: var(--r-sm); }
  button.text:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .title { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); overflow-wrap: anywhere; }
  .detail { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow-wrap: anywhere; }
  .meta { color: var(--faint); font-size: var(--fs-caption); line-height: var(--lh-body-sm); white-space: nowrap; font-variant-numeric: tabular-nums; }
</style>
