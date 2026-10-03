<script lang="ts">
  // Modal surface for dialogs, side sheets and the command palette.
  // - focus is trapped inside and restored to the trigger on close (lib/focus.ts)
  // - Esc and a backdrop click close it when `dismissable`; both are swallowed so nothing
  //   behind the dialog reacts
  // - `placement`: "center" (dialog), "right" (sheet/drawer), "top" (palette)
  import type { Snippet } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import IconButton from "./IconButton.svelte";
  import Icon from "../Icon.svelte";
  import { focusTrap } from "../../lib/focus";
  import { fieldId } from "./Field.svelte";

  let {
    title,
    description,
    icon,
    iconTone = "accent",
    size = "md",
    placement = "center",
    role = "dialog",
    dismissable = true,
    closeButton = true,
    bare = false,
    initialFocus,
    label,
    onclose,
    header,
    children,
    footer,
    surface = $bindable(),
  }: {
    title?: string;
    description?: string;
    /** Icon name for the header tile. */
    icon?: string;
    iconTone?: "accent" | "danger" | "warn" | "ok";
    size?: "sm" | "md" | "lg" | "xl";
    placement?: "center" | "right" | "top";
    role?: "dialog" | "alertdialog";
    /** Esc / backdrop close it (off while an action is running). */
    dismissable?: boolean;
    closeButton?: boolean;
    /** No padding, header or footer chrome: the content owns the whole surface. */
    bare?: boolean;
    /** Selector of the element to focus first. */
    initialFocus?: string;
    /** Accessible name when there's no visible title. */
    label?: string;
    onclose: () => void;
    header?: Snippet;
    children: Snippet;
    footer?: Snippet;
    surface?: HTMLDivElement;
  } = $props();

  const id = fieldId("dlg");
  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const dur = reduced ? 0 : 1;
  let downOnBackdrop = false;

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (dismissable) onclose();
      return;
    }
    // Keys typed inside a dialog never reach the app's global shortcuts.
    e.stopPropagation();
  }
  const enter = $derived(placement === "right" ? { x: 28, y: 0 } : placement === "top" ? { x: 0, y: -8 } : { x: 0, y: 6 });
</script>

<div
  class="ov {placement}"
  role="presentation"
  transition:fade={{ duration: 130 * dur }}
  onpointerdown={(e) => (downOnBackdrop = e.target === e.currentTarget)}
  onclick={(e) => { if (downOnBackdrop && e.target === e.currentTarget && dismissable) onclose(); downOnBackdrop = false; }}
>
  <div
    bind:this={surface}
    class="surface {size}"
    class:bare
    {role}
    aria-modal="true"
    aria-labelledby={title ? `${id}-title` : undefined}
    aria-describedby={description ? `${id}-desc` : undefined}
    aria-label={!title ? label : undefined}
    tabindex="-1"
    use:focusTrap={{ initial: initialFocus }}
    onkeydown={key}
    transition:fly={{ ...enter, duration: (placement === "right" ? 220 : 160) * dur, easing: cubicOut, opacity: placement === "right" ? 1 : 0 }}
  >
    {#if header}
      {@render header()}
    {:else if title && !bare}
      <header class="dh">
        {#if icon}<span class="tile {iconTone}" aria-hidden="true"><Icon name={icon} size={18} /></span>{/if}
        <div class="tt">
          <h2 id="{id}-title">{title}</h2>
          {#if description}<p id="{id}-desc">{description}</p>{/if}
        </div>
        {#if closeButton && dismissable}<IconButton icon="x" label="Close" kbd="Esc" size="sm" onclick={onclose} />{/if}
      </header>
    {/if}
    <div class="db">{@render children()}</div>
    {#if footer}<footer class="df">{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .ov { position: fixed; inset: 0; z-index: 60; background: var(--backdrop); display: flex; justify-content: center; align-items: center; padding: var(--sp-6) var(--sp-4); }
  .ov.center { backdrop-filter: blur(3px) saturate(120%); }
  .ov.top { align-items: flex-start; padding-top: 12vh; backdrop-filter: blur(3px); z-index: 70; }
  .ov.right { justify-content: flex-end; align-items: stretch; padding: 0; }

  .surface { position: relative; display: flex; flex-direction: column; max-height: calc(100vh - 48px); width: min(var(--w), 100%); background: var(--surface); border-radius: var(--r-xl); box-shadow: var(--shadow-lg); outline: none; overflow: hidden; }
  .sm { --w: 420px; } .md { --w: 520px; } .lg { --w: 680px; } .xl { --w: 820px; }
  .top .surface { max-height: 70vh; --w: 640px; }
  .right .surface { max-height: none; height: 100%; width: min(var(--w), 94vw); border-radius: 0; border-left: 1px solid var(--border); }
  .right .md { --w: 460px; }

  .dh { display: flex; align-items: flex-start; gap: var(--sp-3); padding: var(--sp-5) var(--sp-5) var(--sp-3) var(--sp-6); }
  .tile { width: 36px; height: 36px; flex: none; border-radius: var(--r-md); display: grid; place-items: center; }
  .tile.accent { background: var(--accent-soft); color: var(--accent); }
  .tile.danger { background: var(--danger-soft); color: var(--danger); }
  .tile.warn { background: var(--warn-soft); color: var(--warn); }
  .tile.ok { background: var(--ok-soft); color: var(--ok); }
  .tt { flex: 1; min-width: 0; padding-top: 1px; }
  h2 { margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); line-height: var(--lh-title); }
  .tt p { margin: 3px 0 0; color: var(--muted); font-size: var(--fs-body); line-height: var(--lh-body); }
  .db { flex: 1; min-height: 0; overflow-y: auto; padding: var(--sp-2) var(--sp-6) var(--sp-5); display: flex; flex-direction: column; }
  .bare .db { padding: 0; overflow: hidden; }
  .df { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-2); padding: var(--sp-3) var(--sp-5) var(--sp-3) var(--sp-6); border-top: 1px solid var(--border); background: var(--surface-2); }
  :global([data-theme="dark"]) .df { background: color-mix(in srgb, var(--surface-2) 60%, var(--surface)); }
  @media (max-width: 560px) {
    .dh { padding: var(--sp-4) var(--sp-4) var(--sp-2); }
    .db { padding: var(--sp-2) var(--sp-4) var(--sp-4); }
    .df { padding: var(--sp-3) var(--sp-4); flex-wrap: wrap; }
    .ov.center { padding: var(--sp-3); }
  }
</style>
