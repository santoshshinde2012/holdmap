<script lang="ts">
  // Scroll container with edge shadows that appear only when there is more content that way.
  import type { Snippet } from "svelte";

  let { children, label, el = $bindable(), class: cls = "", padded = true }: { children: Snippet; label?: string; el?: HTMLDivElement; class?: string; padded?: boolean } = $props();
  let top = $state(false);
  let bottom = $state(false);

  function update() {
    if (!el) return;
    top = el.scrollTop > 2;
    bottom = el.scrollTop + el.clientHeight < el.scrollHeight - 2;
  }
  $effect(() => {
    if (!el) return;
    update();
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
    ro?.observe(el);
    if (el.firstElementChild) ro?.observe(el.firstElementChild);
    return () => ro?.disconnect();
  });
</script>

<div class="sa" class:top class:bottom>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="vp {cls}" class:padded bind:this={el} onscroll={update} role={label ? "region" : undefined} aria-label={label} tabindex={label ? 0 : undefined}>
    <div class="content">{@render children()}</div>
  </div>
</div>

<style>
  .sa { position: relative; min-height: 0; flex: 1; display: flex; flex-direction: column; }
  .sa::before, .sa::after { content: ""; position: absolute; left: 0; right: 0; height: 14px; pointer-events: none; opacity: 0; transition: opacity var(--dur-2) var(--ease); z-index: 2; }
  .sa::before { top: 0; background: linear-gradient(to bottom, color-mix(in srgb, var(--text) 9%, transparent), transparent); }
  .sa::after { bottom: 0; background: linear-gradient(to top, color-mix(in srgb, var(--text) 9%, transparent), transparent); }
  .sa.top::before, .sa.bottom::after { opacity: 1; }
  :global([data-theme="dark"]) .sa::before { background: linear-gradient(to bottom, rgb(0 0 0 / 0.5), transparent); }
  :global([data-theme="dark"]) .sa::after { background: linear-gradient(to top, rgb(0 0 0 / 0.5), transparent); }
  .vp { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; outline: none; }
  .vp:focus-visible { box-shadow: inset 0 0 0 2px var(--ring); }
  .padded > .content { padding: var(--sp-4) var(--sp-5) var(--sp-6); }
</style>
