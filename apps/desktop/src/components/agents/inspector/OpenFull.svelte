<script lang="ts">
  // "Open full …" link from an inspector tab into the whole-machine workspace.
  import Icon from "../../Icon.svelte";
  let { label, icon, onopen }: { label: string; icon: string; onopen?: () => void } = $props();
</script>

{#if onopen}<button type="button" class="open-full" onclick={onopen}><Icon name={icon} size={13} />{label}<Icon name="arrow" size={12} /></button>{/if}

<style>
  .open-full { justify-self: start; display: inline-flex; align-items: center; gap: 6px; min-height: 30px; padding: 4px 12px; border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border)); border-radius: var(--r-full); background: var(--accent-softer, var(--surface-2)); color: var(--accent-text, var(--accent)); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); cursor: pointer; }
  .open-full:hover { background: var(--accent-soft); }
  .open-full:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
