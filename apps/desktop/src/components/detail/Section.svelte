<script lang="ts">
  // A titled block inside a details tab: consistent heading, optional right-aligned aside.
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  let { title, icon, aside, children }: { title: string; icon?: string; aside?: Snippet; children: Snippet } = $props();
</script>

<section>
  <header>
    <h3>{#if icon}<Icon name={icon} size={13} />{/if}{title}</h3>
    {#if aside}<div class="aside">{@render aside()}</div>{/if}
  </header>
  {@render children()}
</section>

<style>
  section { display: grid; gap: var(--sp-3); }
  section + :global(section) { margin-top: var(--sp-6); }
  header { display: flex; align-items: center; gap: var(--sp-2); min-height: 22px; }
  h3 { margin: 0; flex: 1; display: inline-flex; align-items: center; gap: 7px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text-2); letter-spacing: var(--ls-body-sm); }
  h3 :global(svg) { color: var(--muted); }
  .aside { display: inline-flex; align-items: center; gap: 6px; }
</style>
