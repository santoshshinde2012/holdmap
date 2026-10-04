<script lang="ts" module>
  export interface MenuItem { id: string; label: string; icon: string; hint?: string; run: () => void }
</script>

<script lang="ts">
  // "More actions" button with a small menu (opens upward from the details footer). Arrow keys
  // move, Enter runs, Esc or a click outside closes and returns focus to the button.
  import Icon from "../Icon.svelte";
  import IconButton from "./IconButton.svelte";

  let { items, label = "More actions" }: { items: MenuItem[]; label?: string } = $props();
  let open = $state(false);
  let btn: HTMLButtonElement | undefined = $state();
  let menu: HTMLDivElement | undefined = $state();
  const id = `menu-${Math.random().toString(36).slice(2, 8)}`;

  const buttons = () => [...(menu?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? [])];
  function show() {
    open = true;
    requestAnimationFrame(() => buttons()[0]?.focus());
  }
  function close(focus = true) {
    open = false;
    if (focus) btn?.focus();
  }
  function onkey(e: KeyboardEvent) {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); list[(i + 1) % list.length]?.focus(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); list[(i - 1 + list.length) % list.length]?.focus(); }
    else if (e.key === "Home") { e.preventDefault(); list[0]?.focus(); }
    else if (e.key === "End") { e.preventDefault(); list.at(-1)?.focus(); }
    else if (e.key === "Tab") close(false);
  }
  function outside(e: PointerEvent) {
    if (open && !menu?.contains(e.target as Node) && !btn?.contains(e.target as Node)) close(false);
  }
  function run(item: MenuItem) {
    close();
    item.run();
  }
</script>

<svelte:window onpointerdown={outside} />

<div class="wrap">
  <IconButton bind:el={btn} icon="more" {label} size="md" aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined} onclick={() => (open ? close() : show())} />
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="menu" role="menu" {id} aria-label={label} tabindex="-1" bind:this={menu} onkeydown={onkey}>
      {#each items as item (item.id)}
        <button type="button" role="menuitem" tabindex="-1" onclick={() => run(item)}>
          <Icon name={item.icon} size={14} /><span class="l">{item.label}</span>{#if item.hint}<span class="h">{item.hint}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; display: inline-flex; }
  .menu { position: absolute; bottom: calc(100% + 6px); right: 0; z-index: 30; min-width: 232px; padding: 4px; display: grid; gap: 1px; background: var(--surface-raised, var(--surface)); border: 1px solid var(--border-strong); border-radius: var(--r-lg); box-shadow: var(--shadow-lg, 0 8px 24px rgb(0 0 0 / 0.18)); }
  button { display: flex; align-items: center; gap: 9px; height: 30px; padding: 0 10px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; white-space: nowrap; }
  button :global(svg) { color: var(--muted); flex: none; }
  button:hover, button:focus-visible { background: var(--row-hover); outline: none; }
  .l { flex: 1; }
  .h { color: var(--faint); font-family: var(--mono); font-size: var(--fs-mono-sm); }
</style>
