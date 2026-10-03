<script lang="ts" module>
  let uid = 0;
  /** Stable unique id for a form control. */
  export const fieldId = (prefix = "f") => `${prefix}-${++uid}`;
</script>

<script lang="ts">
  // Label + control + hint/error layout shared by TextField, NumberInput and Select.
  // The control snippet receives the ids it must wire up (aria-describedby / aria-invalid).
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";

  let {
    id,
    label,
    hint,
    error,
    required = false,
    optional = false,
    counter,
    labelHidden = false,
    children,
  }: {
    id: string;
    label?: string;
    hint?: string | null;
    error?: string | null;
    required?: boolean;
    optional?: boolean;
    counter?: string;
    labelHidden?: boolean;
    children: Snippet<[{ id: string; describedBy: string | undefined; invalid: boolean }]>;
  } = $props();

  const describedBy = $derived(error ? `${id}-error` : hint ? `${id}-hint` : undefined);
</script>

<div class="field" class:invalid={!!error}>
  {#if label}
    <div class="top" class:sr-only={labelHidden}>
      <label for={id}>{label}{#if required}<span class="req" aria-hidden="true"> *</span>{/if}</label>
      {#if optional}<span class="opt">Optional</span>{/if}
      {#if counter}<span class="counter">{counter}</span>{/if}
    </div>
  {/if}
  {@render children({ id, describedBy, invalid: !!error })}
  <div class="msgs" aria-live="polite">
    {#if error}<p class="msg err" id="{id}-error"><Icon name="alert" size={12} />{error}</p>
    {:else if hint}<p class="msg" id="{id}-hint">{hint}</p>{/if}
  </div>
</div>

<style>
  .field { display: grid; gap: 6px; min-width: 0; }
  .top { display: flex; align-items: baseline; gap: 8px; }
  label { font-size: var(--fs-sm); font-weight: 550; color: var(--text); }
  .req { color: var(--danger); }
  .opt { font-size: var(--fs-xs); color: var(--muted); }
  .counter { margin-left: auto; font-size: var(--fs-xs); color: var(--muted); font-variant-numeric: tabular-nums; }
  .invalid .counter { color: var(--danger); }
  .msgs:empty { display: none; }
  .msg { margin: 0; font-size: var(--fs-xs); color: var(--muted); line-height: 1.4; display: flex; gap: 6px; align-items: flex-start; }
  .msg :global(svg) { margin-top: 2px; }
  .err { color: var(--danger); font-weight: 500; }
</style>
