<script lang="ts" module>
  export interface SelectOption<T extends string = string> { value: T; label: string; description?: string; icon?: string; disabled?: boolean }
</script>

<script lang="ts" generics="T extends string">
  // Select-only combobox (WAI-ARIA APG): focus stays on the trigger, the list uses
  // aria-activedescendant. ↑/↓/Home/End move, Enter/Space pick, Esc closes, typing jumps.
  import Icon from "../Icon.svelte";
  import Field, { fieldId } from "./Field.svelte";
  import { nextIndex, typeahead } from "../../lib/roving";

  let {
    value = $bindable(),
    options,
    id = fieldId("sel"),
    label,
    hint,
    error,
    prefix,
    size = "md",
    width,
    disabled = false,
    labelHidden = false,
    placeholder = "Select…",
    onchange,
  }: {
    value?: T;
    options: SelectOption<T>[];
    id?: string;
    label?: string;
    hint?: string | null;
    error?: string | null;
    /** Inline label inside the trigger, e.g. "Sort". */
    prefix?: string;
    size?: "sm" | "md";
    width?: string;
    disabled?: boolean;
    labelHidden?: boolean;
    placeholder?: string;
    onchange?: (v: T) => void;
  } = $props();

  let open = $state(false);
  let active = $state(0);
  let trigger: HTMLButtonElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();
  let pos = $state({ top: 0, left: 0, width: 0, up: false });
  let typed = "";
  let typedAt = 0;

  const current = $derived(options.find((o) => o.value === value));
  const listId = $derived(`${id}-list`);
  const disabledFlags = $derived(options.map((o) => !!o.disabled));

  function place() {
    if (!trigger) return;
    const r = trigger.getBoundingClientRect();
    const h = Math.min(320, options.length * 34 + 10);
    const up = r.bottom + h + 8 > innerHeight && r.top > h + 8;
    pos = { top: up ? r.top - h - 4 : r.bottom + 4, left: Math.min(r.left, innerWidth - Math.max(r.width, 180) - 8), width: r.width, up };
  }
  function show() {
    if (disabled) return;
    active = Math.max(0, options.findIndex((o) => o.value === value));
    place();
    open = true;
    requestAnimationFrame(() => list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" }));
  }
  function close(refocus = true) { open = false; if (refocus) trigger?.focus(); }
  function pick(i: number) {
    const o = options[i];
    if (!o || o.disabled) return;
    const changed = o.value !== value;
    value = o.value;
    close();
    if (changed) onchange?.(o.value);
  }
  function key(e: KeyboardEvent) {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) { show(); e.preventDefault(); }
      return;
    }
    if (e.key === "Escape") { close(); e.preventDefault(); e.stopPropagation(); return; }
    if (e.key === "Enter" || e.key === " ") { pick(active); e.preventDefault(); return; }
    if (e.key === "Tab") { pick(active); return; }
    const n = nextIndex(active, e.key, options.length, { orientation: "vertical", loop: false, disabled: disabledFlags });
    if (n !== null) { active = n; e.preventDefault(); scrollActive(); return; }
    if (e.key.length === 1 && /\S/.test(e.key)) {
      const now = Date.now();
      typed = now - typedAt < 600 ? typed + e.key : e.key;
      typedAt = now;
      const t = typeahead(options.map((o) => o.label), typed, typed.length > 1 ? active - 1 : active);
      if (t !== null) { active = t; scrollActive(); }
    }
  }
  function scrollActive() { requestAnimationFrame(() => list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" })); }
  function outside(e: PointerEvent) {
    if (open && !trigger?.contains(e.target as Node) && !list?.contains(e.target as Node)) close(false);
  }
</script>

<svelte:window onpointerdown={outside} onresize={() => open && place()} />

<Field {id} {label} {hint} {error} {labelHidden}>
  {#snippet children({ id, describedBy, invalid })}
    <button
      bind:this={trigger}
      {id}
      type="button"
      class="trigger {size}"
      class:invalid
      class:open
      style={width ? `width: ${width}` : undefined}
      role="combobox"
      aria-haspopup="listbox"
      aria-expanded={open}
      aria-controls={listId}
      aria-activedescendant={open ? `${id}-opt-${active}` : undefined}
      aria-describedby={describedBy}
      aria-invalid={invalid || undefined}
      aria-label={labelHidden || !label ? `${prefix ?? label ?? "Select"}: ${current?.label ?? placeholder}` : undefined}
      {disabled}
      onclick={() => (open ? close() : show())}
      onkeydown={key}
    >
      {#if prefix}<span class="prefix">{prefix}</span>{/if}
      {#if current?.icon}<Icon name={current.icon} size={13} />{/if}
      <span class="val" class:ph={!current}>{current?.label ?? placeholder}</span>
      <Icon name="chevrons-ud" size={13} />
    </button>
    {#if open}
      <div
        bind:this={list}
        class="pop"
        class:up={pos.up}
        id={listId}
        role="listbox"
        aria-label={label ?? prefix}
        tabindex="-1"
        style="top: {pos.top}px; left: {pos.left}px; min-width: {Math.max(pos.width, 180)}px"
      >
        {#each options as o, i}
          <div
            id="{id}-opt-{i}"
            data-i={i}
            role="option"
            tabindex="-1"
            class="opt"
            class:active={i === active}
            class:disabled={o.disabled}
            aria-selected={o.value === value}
            aria-disabled={o.disabled || undefined}
            onpointermove={() => !o.disabled && (active = i)}
            onpointerdown={(e) => e.preventDefault()}
            onclick={() => pick(i)}
            onkeydown={() => {}}
          >
            {#if o.icon}<span class="oi"><Icon name={o.icon} size={14} /></span>{/if}
            <span class="ot"><span>{o.label}</span>{#if o.description}<span class="od">{o.description}</span>{/if}</span>
            {#if o.value === value}<span class="check"><Icon name="check" size={14} /></span>{/if}
          </div>
        {/each}
      </div>
    {/if}
  {/snippet}
</Field>

<style>
  .trigger { display: inline-flex; align-items: center; gap: 8px; height: var(--h-md); padding: 0 8px 0 10px; min-width: 0; border-radius: var(--r-md); border: 1px solid var(--input-border); background: var(--input-bg); color: var(--muted); box-shadow: var(--control-shadow); font: inherit; font-size: var(--fs-body); line-height: var(--lh-body); cursor: pointer; transition: border-color var(--dur-1), box-shadow var(--dur-1); text-align: left; }
  .trigger.sm { height: var(--h-sm); border-radius: var(--r-sm); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .trigger:hover:not(:disabled) { border-color: var(--input-border-hover); }
  .trigger:focus-visible, .trigger.open { outline: none; border-color: var(--accent); box-shadow: var(--focus-ring); }
  .trigger.invalid { border-color: var(--danger); }
  .trigger:disabled { opacity: 0.55; cursor: not-allowed; background: var(--input-disabled); }
  .prefix { color: var(--muted); }
  .val { flex: 1; min-width: 0; color: var(--text); font-weight: var(--fw-medium); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .val.ph { color: var(--faint); font-weight: var(--fw-regular); }
  .pop { position: fixed; z-index: 200; max-height: 320px; overflow-y: auto; padding: 4px; border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--border-strong); box-shadow: var(--shadow-lg); outline: none; animation: pop var(--dur-1) var(--ease); }
  .pop.up { animation-name: popup; }
  @keyframes pop { from { opacity: 0; transform: translateY(-3px); } }
  @keyframes popup { from { opacity: 0; transform: translateY(3px); } }
  .opt { display: flex; align-items: center; gap: 8px; min-height: 32px; padding: 6px 8px; border-radius: var(--r-sm); font-size: var(--fs-body); line-height: var(--lh-body); color: var(--text); cursor: default; }
  .opt.active { background: var(--row-selected); }
  .opt.disabled { opacity: 0.45; }
  .oi { color: var(--muted); display: inline-grid; }
  .ot { flex: 1; display: grid; min-width: 0; }
  .od { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .check { color: var(--accent); display: inline-grid; }
</style>
