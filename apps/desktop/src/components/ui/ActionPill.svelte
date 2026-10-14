<script lang="ts">
  // Single action in the dock language: command (outline) or independent toggle (soft fill).
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";

  let {
    icon,
    label,
    name,
    pressed = false,
    toggle = false,
    disabled = false,
    title,
    kbd,
    size = "md",
    expanded,
    onclick,
  }: {
    icon?: string;
    label: string;
    /** Accessible name when it differs from the visible label. */
    name?: string;
    pressed?: boolean;
    /** Soft accent when on, for Hide/Show and canvas filters. Exclusive groups use ActionDock. */
    toggle?: boolean;
    disabled?: boolean;
    title?: string;
    kbd?: string;
    size?: "md" | "compact";
    expanded?: boolean;
    onclick?: (e: MouseEvent) => void;
  } = $props();
</script>

<button
  type="button"
  class="action-pill"
  class:toggle
  class:on={pressed}
  class:compact={size === "compact"}
  {disabled}
  aria-label={name}
  aria-pressed={toggle ? pressed : undefined}
  aria-expanded={expanded}
  title={title}
  use:tooltip={title ? { text: title, kbd, placement: "bottom" } : kbd ? { text: label, kbd, placement: "bottom" } : null}
  {onclick}
>
  {#if icon}<Icon name={icon} size={12} />{/if}<span>{label}</span>
</button>
