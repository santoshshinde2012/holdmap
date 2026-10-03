<script lang="ts">
  // Native checkbox (keyboard + form semantics for free) with a custom box, label and description.
  import { fieldId } from "./Field.svelte";

  let {
    checked = $bindable(false),
    id = fieldId("cb"),
    label,
    description,
    disabled = false,
    tone = "default",
    onchange,
  }: {
    checked?: boolean;
    id?: string;
    label: string;
    description?: string;
    disabled?: boolean;
    /** `danger` for confirmations of destructive actions. */
    tone?: "default" | "danger";
    onchange?: (checked: boolean) => void;
  } = $props();
</script>

<div class="cb {tone}" class:disabled>
  <span class="boxwrap">
    <input type="checkbox" {id} bind:checked {disabled} aria-describedby={description ? `${id}-desc` : undefined} onchange={() => onchange?.(checked)} />
    <svg class="mark" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 5" /></svg>
  </span>
  <div class="text">
    <label for={id}>{label}</label>
    {#if description}<p id="{id}-desc">{description}</p>{/if}
  </div>
</div>

<style>
  .cb { display: flex; gap: 10px; align-items: flex-start; }
  .boxwrap { position: relative; flex: none; width: 16px; height: 16px; margin-top: 1px; }
  input { appearance: none; -webkit-appearance: none; margin: 0; width: 16px; height: 16px; border-radius: var(--r-xs); border: 1px solid var(--input-border-hover); background: var(--input-bg); box-shadow: var(--control-shadow); cursor: pointer; transition: background var(--dur-1), border-color var(--dur-1); display: block; }
  input:hover:not(:disabled) { border-color: var(--accent); }
  input:checked { background: var(--accent); border-color: var(--accent); }
  .danger input:checked { background: var(--danger); border-color: var(--danger); }
  .danger .mark { stroke: var(--danger-fg); }
  input:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  input:disabled { opacity: 0.5; cursor: not-allowed; }
  .mark { position: absolute; inset: 0; width: 16px; height: 16px; fill: none; stroke: var(--accent-fg); stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; pointer-events: none; opacity: 0; transform: scale(0.7); transition: opacity var(--dur-1), transform var(--dur-2) var(--ease-spring); }
  input:checked + .mark { opacity: 1; transform: none; }
  .text { min-width: 0; }
  label { font-size: var(--fs-body); font-weight: var(--fw-medium); color: var(--text); cursor: pointer; line-height: var(--lh-body); }
  p { margin: 2px 0 0; font-size: var(--fs-body-sm); color: var(--muted); line-height: var(--lh-body-sm); }
  .disabled label { color: var(--muted); cursor: default; }
</style>
