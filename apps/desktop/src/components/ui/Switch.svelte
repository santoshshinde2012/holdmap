<script lang="ts">
  // Toggle switch (role="switch"). With a label it renders a full settings row: text on the
  // left, switch on the right; clicking the label toggles it.
  import { fieldId } from "./Field.svelte";

  let {
    checked = $bindable(false),
    id = fieldId("sw"),
    label,
    description,
    disabled = false,
    busy = false,
    size = "md",
    labelledby,
    describedby,
    onchange,
  }: {
    checked?: boolean;
    id?: string;
    label?: string;
    description?: string;
    disabled?: boolean;
    /** Saving: keeps the switch disabled and shows progress. */
    busy?: boolean;
    size?: "sm" | "md";
    /** Ids of an external label / description (when the switch is used bare, e.g. in a SettingRow). */
    labelledby?: string;
    describedby?: string;
    onchange?: (checked: boolean) => void;
  } = $props();

  function toggle() {
    if (disabled || busy) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<div class="row" class:disabled class:bare={!label}>
  {#if label}
    <div class="text">
      <label for={id} id="{id}-label">{label}</label>
      {#if description}<p id="{id}-desc">{description}</p>{/if}
    </div>
  {/if}
  <button
    {id}
    type="button"
    role="switch"
    class="sw {size}"
    class:on={checked}
    class:busy
    aria-checked={checked}
    aria-labelledby={label ? `${id}-label` : labelledby}
    aria-describedby={description ? `${id}-desc` : describedby}
    aria-busy={busy || undefined}
    disabled={disabled}
    onclick={toggle}
  ><span class="thumb"></span></button>
</div>

<style>
  .row { display: flex; align-items: center; gap: var(--sp-4); justify-content: space-between; min-width: 0; }
  .row.bare { display: inline-flex; }
  .text { min-width: 0; }
  label { font-size: var(--fs-sm); font-weight: 550; color: var(--text); cursor: pointer; }
  p { margin: 2px 0 0; font-size: var(--fs-xs); color: var(--muted); line-height: 1.45; }
  .disabled label { cursor: default; color: var(--muted); }
  .sw { --w: 34px; --h: 20px; position: relative; flex: none; width: var(--w); height: var(--h); border-radius: 999px; border: 0; padding: 0; background: var(--surface-3); box-shadow: inset 0 0 0 1px var(--border-strong); cursor: pointer; transition: background var(--dur-2) var(--ease), box-shadow var(--dur-2); }
  .sw.sm { --w: 28px; --h: 16px; }
  .sw:hover:not(:disabled) { box-shadow: inset 0 0 0 1px var(--input-border-hover); }
  .sw.on { background: var(--accent); box-shadow: none; }
  .sw:disabled { opacity: 0.5; cursor: not-allowed; }
  .sw.busy { cursor: progress; opacity: 0.75; }
  .sw:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .thumb { position: absolute; top: 2px; left: 2px; width: calc(var(--h) - 4px); height: calc(var(--h) - 4px); border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgb(0 0 0 / 0.3); transition: transform var(--dur-2) var(--ease-spring); }
  .on .thumb { transform: translateX(calc(var(--w) - var(--h))); }
</style>
