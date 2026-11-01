<script lang="ts">
  // A titled panel whose body folds away under its header, and can be closed when the place it
  // sits in allows. The header is a heading holding the toggle (the WAI-ARIA disclosure
  // pattern), so the panel's name and state read the same to everyone. With `pref`, the panel
  // remembers whether it was open.
  import { untrack, type Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  import { prefGetBoolean, prefSet } from "../../lib/prefs";

  let { id, title, count = null, tone = "warn", pref, open = $bindable(true), fill = false, onclose, actions, children }: {
    /** Prefix for the heading and body ids. */
    id: string;
    title: string;
    /** A badge after the title, hidden at 0 and while unknown. */
    count?: number | null;
    tone?: "warn" | "ok" | "neutral";
    /** Remembers the open state under this preference. */
    pref?: string;
    open?: boolean;
    /** Open, the body takes the panel's spare height (a list that scrolls inside it). */
    fill?: boolean;
    /** Shows a close button; the owner decides what closing means. */
    onclose?: () => void;
    /** Small controls at the right of the header. */
    actions?: Snippet;
    children: Snippet;
  } = $props();

  // A remembered state wins over the default the owner passed (read once, when the panel mounts).
  untrack(() => { if (pref) open = prefGetBoolean(pref, open); });
  $effect(() => { if (pref) prefSet(pref, open ? "1" : "0"); });
</script>

<section class="cpanel" class:closed={!open} class:fill aria-labelledby="{id}-h" data-capture-area>
  <header>
    <h2 id="{id}-h">
      <button type="button" class="toggle" aria-expanded={open} aria-controls="{id}-b" title="{open ? 'Collapse' : 'Expand'} {title}" onclick={() => (open = !open)}>
        <span class="chev" aria-hidden="true"><Icon name="chevron-down" size={14} /></span>
        <span class="t">{title}</span>{#if count}{" "}<span class="count {tone}">{count}</span>{/if}
      </button>
    </h2>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
    {#if onclose}
      <button type="button" class="x" aria-label="Close {title}" title="Close {title}" onclick={onclose}><Icon name="x" size={13} /></button>
    {/if}
  </header>
  <div class="body" id="{id}-b" hidden={!open}>{@render children()}</div>
</section>

<style>
  .cpanel { display: flex; flex-direction: column; min-height: 0; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: clip; }
  .cpanel.fill:not(.closed) { flex: 1 1 auto; }
  header { display: flex; align-items: center; gap: var(--sp-1); padding: 4px 6px 4px 4px; border-bottom: 1px solid var(--border); }
  .closed header { border-bottom-color: transparent; }
  h2 { flex: 1; min-width: 0; margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  .toggle { display: flex; align-items: center; gap: var(--sp-2); width: 100%; min-height: 32px; padding: 4px 6px; border: 0; border-radius: var(--r-md); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .toggle:hover { background: var(--row-hover); }
  .toggle:focus-visible, .x:focus-visible { outline: 2px solid var(--ring); outline-offset: -1px; }
  .chev { display: inline-flex; color: var(--muted); transition: transform 160ms ease; }
  .closed .chev { transform: rotate(-90deg); }
  @media (prefers-reduced-motion: reduce) { .chev { transition: none; } }
  .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { padding: 0 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .count.warn { background: var(--warn-soft); color: var(--warn); }
  .count.ok { background: var(--ok-soft); color: var(--ok); }
  .count.neutral { background: var(--surface-3); color: var(--text-2); }
  .actions { display: flex; align-items: center; gap: 2px; }
  .x { display: inline-grid; place-items: center; width: 26px; height: 26px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--muted); cursor: pointer; }
  .x:hover { background: var(--row-hover); color: var(--text); }
  .body { min-height: 0; display: flex; flex-direction: column; }
  .fill .body { flex: 1 1 auto; }
  .body[hidden] { display: none; }
</style>
