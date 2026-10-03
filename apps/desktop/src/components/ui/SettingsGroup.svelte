<script lang="ts">
  import type { Snippet } from "svelte";
  let { title, description, children }: { title?: string; description?: string; children: Snippet } = $props();
</script>

<section class="sg">
  {#if title}<h3>{title}</h3>{/if}
  {#if description}<p>{description}</p>{/if}
  <div class="card">{@render children()}</div>
</section>

<style>
  :global(section.sg + section.sg) { margin-top: var(--sp-6); }
  h3 { margin: 0 0 2px; font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); }
  p { margin: 0 0 var(--sp-3); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  h3 + .card { margin-top: var(--sp-3); }
  .card { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
</style>
