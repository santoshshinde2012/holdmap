<script lang="ts">
  // Numeric spin button: type, use ↑/↓ (Shift ×10) or the steppers; out-of-range input shows an
  // inline error and is never committed. `onchange` fires with valid values only.
  import Icon from "../Icon.svelte";
  import Field, { fieldId } from "./Field.svelte";
  import { validateRange } from "../../lib/validate";

  let {
    value = $bindable(null),
    id = fieldId("num"),
    label,
    hint,
    error: externalError,
    min = 0,
    max = 100,
    step = 1,
    unit,
    size = "md",
    width = "120px",
    required = false,
    disabled = false,
    labelHidden = false,
    placeholder,
    onchange,
  }: {
    value?: number | null;
    id?: string;
    label?: string;
    hint?: string | null;
    error?: string | null;
    min?: number;
    max?: number;
    step?: number;
    /** Suffix such as "s" or "entries". */
    unit?: string;
    size?: "sm" | "md";
    width?: string;
    required?: boolean;
    disabled?: boolean;
    labelHidden?: boolean;
    placeholder?: string;
    onchange?: (v: number) => void;
  } = $props();

  let text = $state(value === null ? "" : String(value));
  let editing = $state(false);
  let touched = $state(false);
  $effect(() => { if (!editing) text = value === null ? "" : String(value); });

  const parsed = $derived(text.trim() === "" ? null : Number(text));
  // Only complain once the user has typed or left the field.
  const localError = $derived(!touched || (text.trim() === "" && !required) ? null : validateRange(parsed, min, max, unit ? ` ${unit}` : ""));
  const error = $derived(externalError ?? localError);

  function commit(v: number | null) {
    if (v === null || validateRange(v, min, max)) return;
    value = v;
    text = String(v);
    onchange?.(v);
  }
  function bump(dir: number, big = false) {
    const base = parsed !== null && !Number.isNaN(parsed) ? parsed : value ?? min;
    commit(Math.min(max, Math.max(min, Math.round(base + dir * step * (big ? 10 : 1)))));
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowUp") { bump(1, e.shiftKey); e.preventDefault(); }
    else if (e.key === "ArrowDown") { bump(-1, e.shiftKey); e.preventDefault(); }
    else if (e.key === "Home") { commit(min); e.preventDefault(); }
    else if (e.key === "End") { commit(max); e.preventDefault(); }
    else if (e.key === "Enter") commit(parsed);
  }
</script>

<Field {id} {label} {hint} {error} {required} {labelHidden}>
  {#snippet children({ id, describedBy, invalid })}
    <div class="ctl {size}" class:invalid class:disabled style="width: {width}">
      <input
        {id}
        {disabled}
        {placeholder}
        bind:value={text}
        inputmode="numeric"
        role="spinbutton"
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value ?? undefined}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onfocus={() => (editing = true)}
        oninput={() => (touched = true)}
        onblur={() => { editing = false; if (text.trim() !== "") touched = true; commit(parsed); }}
        onkeydown={key}
      />
      {#if unit}<span class="unit" aria-hidden="true">{unit}</span>{/if}
      <span class="steps">
        <button type="button" tabindex="-1" aria-label="Increase" disabled={disabled || (value ?? min) >= max} onclick={() => bump(1)}><Icon name="chevron-up" size={11} /></button>
        <button type="button" tabindex="-1" aria-label="Decrease" disabled={disabled || (value ?? min) <= min} onclick={() => bump(-1)}><Icon name="chevron-down" size={11} /></button>
      </span>
    </div>
  {/snippet}
</Field>

<style>
  .ctl { display: flex; align-items: center; height: var(--h-md); padding-left: 10px; background: var(--input-bg); border: 1px solid var(--input-border); border-radius: var(--r-md); box-shadow: var(--control-shadow); overflow: hidden; transition: border-color var(--dur-1), box-shadow var(--dur-1); }
  .ctl.sm { height: var(--h-sm); border-radius: var(--r-sm); }
  .ctl:hover:not(.disabled) { border-color: var(--input-border-hover); }
  .ctl:focus-within { border-color: var(--accent); box-shadow: var(--focus-ring); }
  .ctl.invalid { border-color: var(--danger); }
  .ctl.invalid:focus-within { box-shadow: var(--danger-ring); }
  .ctl.disabled { background: var(--input-disabled); opacity: 0.7; }
  input { flex: 1; min-width: 0; border: 0; outline: 0; padding: 0; background: transparent; color: var(--text); font: inherit; font-size: var(--fs-base); font-variant-numeric: tabular-nums; user-select: text; -webkit-user-select: text; }
  .unit { color: var(--muted); font-size: var(--fs-sm); padding: 0 8px 0 4px; }
  .steps { display: grid; grid-template-rows: 1fr 1fr; align-self: stretch; border-left: 1px solid var(--input-border); width: 22px; flex: none; }
  .steps button { border: 0; background: transparent; color: var(--muted); display: grid; place-items: center; cursor: pointer; padding: 0; }
  .steps button + button { border-top: 1px solid var(--input-border); }
  .steps button:hover:not(:disabled) { background: var(--surface-2); color: var(--text); }
  .steps button:disabled { opacity: 0.35; cursor: default; }
</style>
