<script lang="ts">
  // Text input with label, hint, error, leading icon, clear button, trailing slot and an
  // optional character counter. Esc clears a clearable field before anything else handles it.
  import type { Snippet } from "svelte";
  import type { HTMLInputAttributes } from "svelte/elements";
  import Icon from "../Icon.svelte";
  import Kbd from "./Kbd.svelte";
  import Field, { fieldId } from "./Field.svelte";

  let {
    value = $bindable(""),
    input = $bindable(),
    id = fieldId("tf"),
    label,
    hint,
    error,
    icon,
    size = "md",
    clearable = false,
    kbdHint,
    mono = false,
    maxlength,
    counter = false,
    required = false,
    optional = false,
    labelHidden = false,
    disabled = false,
    readonly = false,
    variant = "default",
    trailing,
    onclear,
    onkeydown,
    onfocus,
    onblur,
    class: cls = "",
    ...rest
  }: Omit<HTMLInputAttributes, "value" | "size" | "class"> & {
    value?: string;
    input?: HTMLInputElement;
    id?: string;
    label?: string;
    hint?: string | null;
    error?: string | null;
    icon?: string;
    size?: "sm" | "md" | "lg";
    clearable?: boolean;
    /** Shortcut shown while the field is empty and unfocused (e.g. "/"). */
    kbdHint?: string;
    mono?: boolean;
    maxlength?: number;
    counter?: boolean;
    required?: boolean;
    optional?: boolean;
    labelHidden?: boolean;
    disabled?: boolean;
    readonly?: boolean;
    /** `filled` sits on a toolbar surface (search), `default` on cards and dialogs. */
    variant?: "default" | "filled";
    trailing?: Snippet;
    onclear?: () => void;
    class?: string;
  } = $props();

  let focused = $state(false);
  const len = $derived([...(value ?? "")].length);

  function clear() {
    value = "";
    onclear?.();
    input?.focus();
  }
  function key(e: KeyboardEvent & { currentTarget: EventTarget & HTMLInputElement }) {
    if (e.key === "Escape" && clearable && value) { clear(); e.preventDefault(); e.stopPropagation(); return; }
    onkeydown?.(e);
  }
  // Clicking the padding/icon focuses the input, like a native field.
  function down(e: PointerEvent) {
    if (e.target instanceof HTMLElement && !e.target.closest("input,button")) { e.preventDefault(); input?.focus(); }
  }
</script>

<Field {id} {label} {hint} {error} {required} {optional} {labelHidden} counter={counter && maxlength ? `${len}/${maxlength}` : undefined}>
  {#snippet children({ id, describedBy, invalid })}
    <div class="ctl {size} {variant} {cls}" class:invalid class:disabled class:readonly class:focused onpointerdown={down} role="presentation">
      {#if icon}<span class="lead" aria-hidden="true"><Icon name={icon} size={size === "sm" ? 13 : 15} /></span>{/if}
      <input
        bind:this={input}
        bind:value
        {id}
        {disabled}
        {readonly}
        {required}
        {maxlength}
        class:mono
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onkeydown={key}
        onfocus={(e) => { focused = true; onfocus?.(e); }}
        onblur={(e) => { focused = false; onblur?.(e); }}
        {...rest}
      />
      {#if clearable && value && !disabled && !readonly}
        <button type="button" class="clear" aria-label={label ? `Clear ${label.toLowerCase()}` : "Clear"} tabindex="-1" onclick={clear}><Icon name="x" size={12} /></button>
      {:else if kbdHint && !focused}
        <Kbd keys={kbdHint} size="sm" />
      {/if}
      {#if trailing}{@render trailing()}{/if}
    </div>
  {/snippet}
</Field>

<style>
  .ctl {
    --ch: var(--h-md);
    display: flex; align-items: center; gap: 8px; height: var(--ch); padding: 0 6px 0 10px; min-width: 0;
    background: var(--input-bg); border: 1px solid var(--input-border); border-radius: var(--r-md);
    box-shadow: var(--control-shadow); color: var(--muted); cursor: text;
    transition: border-color var(--dur-1) var(--ease), box-shadow var(--dur-1) var(--ease), background var(--dur-1);
  }
  .ctl.sm { --ch: var(--h-sm); padding-left: 8px; border-radius: var(--r-sm); }
  .ctl.lg { --ch: var(--h-lg); padding-left: 12px; }
  .ctl.filled { background: var(--surface-2); border-color: var(--border); box-shadow: none; }
  .ctl:hover:not(.disabled) { border-color: var(--input-border-hover); }
  .ctl.focused, .ctl:focus-within { border-color: var(--accent); box-shadow: var(--focus-ring); background: var(--input-bg); color: var(--text-2); }
  .ctl.invalid { border-color: var(--danger); }
  .ctl.invalid:focus-within { box-shadow: var(--danger-ring); }
  .ctl.disabled { background: var(--input-disabled); cursor: not-allowed; opacity: 0.7; box-shadow: none; }
  .ctl.readonly { background: var(--surface-2); }
  .lead { display: inline-grid; place-items: center; flex: none; }
  input { flex: 1; min-width: 0; height: 100%; border: 0; outline: 0; padding: 0; background: transparent; color: var(--text); font: inherit; font-size: var(--fs-body); line-height: var(--lh-body); user-select: text; -webkit-user-select: text; }
  .sm input { font-size: var(--fs-body); line-height: var(--lh-body); }
  input.mono { font-family: var(--mono); font-size: var(--fs-mono); line-height: var(--lh-mono); }
  input::placeholder { color: var(--faint); opacity: 1; }
  input:disabled { cursor: not-allowed; }
  input::-webkit-search-cancel-button, input::-webkit-search-decoration { display: none; -webkit-appearance: none; }
  .clear { flex: none; width: 20px; height: 20px; display: grid; place-items: center; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--muted); cursor: pointer; }
  .clear:hover { background: var(--surface-3); color: var(--text); }
</style>
