<script lang="ts">
  // One preference: label + description on the left, its control on the right. The control
  // snippet gets the ids to reference (aria-labelledby / aria-describedby).
  import type { Snippet } from "svelte";
  import { fieldId } from "./Field.svelte";
  let { label, description, stack = false, children }: { label: string; description?: string; stack?: boolean; children: Snippet<[{ labelId: string; descId: string | undefined }]> } = $props();
  const id = fieldId("set");
</script>

<div class="row" class:stack>
  <div class="text">
    <div class="l" id="{id}-l">{label}</div>
    {#if description}<p id="{id}-d">{description}</p>{/if}
  </div>
  <div class="ctl">{@render children({ labelId: `${id}-l`, descId: description ? `${id}-d` : undefined })}</div>
</div>

<style>
  .row { display: flex; align-items: center; gap: var(--sp-6); padding: 14px 16px; min-height: 60px; }
  .row + :global(.row) { border-top: 1px solid var(--border); }
  .row.stack { flex-direction: column; align-items: stretch; gap: var(--sp-3); }
  .text { flex: 1; min-width: 0; }
  .l { font-size: var(--fs-sm); font-weight: 550; color: var(--text); }
  p { margin: 2px 0 0; font-size: var(--fs-xs); color: var(--muted); line-height: 1.5; }
  .ctl { flex: none; display: flex; align-items: center; gap: 8px; }
  .stack .ctl { flex: 1; }
  @media (max-width: 560px) { .row { flex-direction: column; align-items: stretch; gap: var(--sp-3); } }
</style>
