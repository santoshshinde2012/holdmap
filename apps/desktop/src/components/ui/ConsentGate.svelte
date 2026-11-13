<script lang="ts">
  // The single consent action for reading agent configuration: one label and one sentence.
  import Icon from "../Icon.svelte";
  import { CONSENT_LABEL, CONSENT_SENTENCE } from "../../lib/consent";

  let { onconsent, compact = false }: { onconsent: () => void; compact?: boolean } = $props();
</script>

<span class="consent" class:compact>
  <button type="button" onclick={onconsent} title={CONSENT_SENTENCE}><Icon name="plug" size={12} />{CONSENT_LABEL}</button>
  {#if !compact}<span class="why">{CONSENT_SENTENCE}</span>{/if}
</span>

<style>
  .consent { display: inline-flex; flex-wrap: wrap; align-items: center; gap: 4px 8px; min-width: 0; }
  button { display: inline-flex; align-items: center; gap: 5px; min-height: 26px; padding: 2px 10px; border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border)); border-radius: var(--r-full); background: var(--accent-softer, var(--surface-2)); color: var(--accent-text, var(--accent)); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); cursor: pointer; white-space: nowrap; }
  button:hover { background: var(--accent-soft); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .why { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
</style>
