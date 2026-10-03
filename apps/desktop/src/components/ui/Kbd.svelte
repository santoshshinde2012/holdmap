<script lang="ts">
  // Keyboard hint: one or more keys, e.g. <Kbd keys={["⌘", "K"]} />.
  let { keys, size = "md", tone = "default" }: { keys: string | string[]; size?: "sm" | "md"; tone?: "default" | "inverse" } = $props();
  const list = $derived(Array.isArray(keys) ? keys : [keys]);
</script>

<span class="kbds {size} {tone}" aria-hidden="true">{#each list as k}<kbd>{k}</kbd>{/each}</span>

<style>
  .kbds { display: inline-flex; gap: 3px; align-items: center; flex: none; }
  .sm kbd { height: 16px; min-width: 16px; font-size: 10px; padding: 0 4px; border-bottom-width: 1px; }
  .inverse kbd { background: rgb(255 255 255 / 0.18); border-color: transparent; color: inherit; }
</style>
