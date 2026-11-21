<script lang="ts">
  // One destination from the agent details screen. The hint says what that screen is for.
  import Icon from "../../Icon.svelte";
  let { label, icon, hint, onopen }: { label: string; icon: string; hint?: string; onopen?: () => void } = $props();
</script>

{#if onopen}
  <button type="button" class="open-full" onclick={onopen} title={hint}>
    <Icon name={icon} size={13} />
    <span class="copy"><b>{label}</b>{#if hint}<small aria-hidden="true">{hint}</small>{/if}</span>
    <Icon name="arrow" size={12} />
  </button>
{/if}

<style>
  .open-full { width: 100%; display: flex; align-items: center; gap: 8px; min-height: 40px; padding: 6px 12px; border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border)); border-radius: var(--r-md); background: var(--accent-softer, var(--surface-2)); color: var(--accent-text, var(--accent)); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); text-align: left; cursor: pointer; }
  .open-full:hover { background: var(--accent-soft); }
  .open-full:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .copy { display: grid; flex: 1; min-width: 0; }
  .copy b { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); }
  .copy small { color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); letter-spacing: normal; text-transform: none; }
</style>
