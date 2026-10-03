<script lang="ts">
  import { slide } from "svelte/transition";
  import Icon from "./Icon.svelte";
  let { mod, ondismiss }: { mod: string; ondismiss: () => void } = $props();
  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
</script>

<div class="onb" role="note" aria-label="Getting started" out:slide={{ duration: reduced ? 0 : 180 }}>
  <div class="art" aria-hidden="true"><Icon name="sparkles" size={18} /></div>
  <div class="body">
    <strong>Welcome to portwise</strong>
    <p>Every port in use, who owns it, and a safe way to stop it.</p>
    <ul>
      <li><kbd>{mod}</kbd><kbd>K</kbd> command palette</li>
      <li><kbd>/</kbd> search — try <code>:3000</code> or a project name</li>
      <li><kbd>⌫</kbd> stop the selected port (always asks first)</li>
    </ul>
  </div>
  <button class="btn sm" onclick={ondismiss}>Got it</button>
</div>

<style>
  .onb { display: flex; gap: var(--sp-3); align-items: flex-start; margin: var(--sp-3) var(--sp-4) var(--sp-1); padding: var(--sp-4); border-radius: var(--r-lg); background: linear-gradient(135deg, var(--accent-soft), var(--accent-softer)); border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent); }
  .art { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; background: var(--accent); color: var(--accent-fg); flex: none; }
  .body { flex: 1; min-width: 0; }
  strong { font-size: var(--fs-heading); font-weight: var(--fw-semibold); letter-spacing: var(--ls-heading); line-height: var(--lh-heading); }
  p { margin: 2px 0 var(--sp-2); color: var(--text-2); }
  ul { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: var(--sp-2) var(--sp-5); color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); }
  li { display: inline-flex; align-items: center; gap: 4px; }
  code { background: var(--surface); padding: 0 4px; border-radius: 4px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
</style>
