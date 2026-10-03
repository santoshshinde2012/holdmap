<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import Icon from "./Icon.svelte";
  import { rank, type Command } from "../lib/palette";

  let { commands, onclose }: { commands: Command[]; onclose: () => void } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  let query = $state("");
  let active = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  const results = $derived(rank(query, commands, 50));
  const groups = $derived.by(() => {
    if (query.trim()) return [{ name: "Results", items: results }];
    const order = ["Ports", "Actions", "Filters", "View", "Settings"] as const;
    return order.map((g) => ({ name: g, items: results.filter((c) => c.group === g) })).filter((g) => g.items.length);
  });
  const flat = $derived(groups.flatMap((g) => g.items));

  $effect(() => { input?.focus(); });
  $effect(() => { void query; active = 0; });

  function run(c: Command) {
    onclose();
    queueMicrotask(c.run);
  }

  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) { active = Math.min(flat.length - 1, active + 1); scroll(); e.preventDefault(); }
    else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) { active = Math.max(0, active - 1); scroll(); e.preventDefault(); }
    else if (e.key === "Enter") { const c = flat[active]; if (c) run(c); e.preventDefault(); }
    else if (e.key === "Escape") { onclose(); e.preventDefault(); }
    e.stopPropagation();
  }
  function scroll() {
    requestAnimationFrame(() => listEl?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" }));
  }
</script>

<div class="backdrop" role="presentation" transition:fade={{ duration: reduced ? 0 : 100 }} onclick={onclose}>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette" tabindex="-1" in:fly={{ y: reduced ? 0 : -6, duration: reduced ? 0 : 150 }} onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <div class="search">
      <Icon name="command" size={16} />
      <input
        bind:this={input}
        bind:value={query}
        placeholder="Type a command, a port, or a project…"
        spellcheck="false"
        autocomplete="off"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={flat[active] ? `cmd-${flat[active].id}` : undefined}
      />
      <kbd>Esc</kbd>
    </div>
    <div class="list" id="palette-list" role="listbox" bind:this={listEl}>
      {#if !flat.length}
        <div class="none">No commands match “{query}”.<br /><span>Tip: type a port number like <code>3000</code> to check it.</span></div>
      {/if}
      {#each groups as g}
        <div class="gh" role="presentation">{g.name}</div>
        {#each g.items as c}
          {@const i = flat.indexOf(c)}
          <div
            class="item"
            class:active={i === active}
            class:danger={c.danger}
            role="option"
            id={"cmd-" + c.id}
            aria-selected={i === active}
            tabindex="-1"
            data-i={i}
            onmousemove={() => (active = i)}
            onclick={() => run(c)}
            onkeydown={() => {}}
          >
            <span class="ic"><Icon name={c.icon ?? "arrow"} size={15} /></span>
            <span class="t">{c.title}{#if c.subtitle}<span class="s">{c.subtitle}</span>{/if}</span>
            {#if c.shortcut}<span class="keys">{#each c.shortcut as k}<kbd>{k}</kbd>{/each}</span>{/if}
          </div>
        {/each}
      {/each}
    </div>
    <div class="foot"><span><kbd>↑</kbd><kbd>↓</kbd> navigate</span><span><kbd>↵</kbd> run</span><span><kbd>Esc</kbd> close</span></div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: var(--backdrop); backdrop-filter: blur(3px); z-index: 70; display: flex; justify-content: center; align-items: flex-start; padding: 12vh var(--sp-4) var(--sp-4); }
  .palette { width: min(620px, 100%); background: var(--surface); border-radius: var(--r-xl); box-shadow: var(--shadow-lg); overflow: hidden; outline: none; display: flex; flex-direction: column; max-height: 70vh; }
  .search { display: flex; align-items: center; gap: var(--sp-3); padding: 0 var(--sp-4); height: 54px; border-bottom: 1px solid var(--border); color: var(--muted); }
  .search input { flex: 1; border: 0; outline: 0; background: transparent; font-size: var(--fs-lg); color: var(--text); }
  .search input::placeholder { color: var(--faint); }
  .list { overflow-y: auto; padding: var(--sp-2); }
  .gh { font-size: var(--fs-2xs); text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); font-weight: 700; padding: var(--sp-2) var(--sp-3) var(--sp-1); }
  .item { display: flex; align-items: center; gap: var(--sp-3); height: 40px; padding: 0 var(--sp-3); border-radius: var(--r-md); cursor: default; }
  .item.active { background: var(--accent-soft); }
  .ic { width: 26px; height: 26px; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--surface-2); color: var(--muted); flex: none; }
  .item.active .ic { background: var(--accent); color: var(--accent-fg); }
  .item.danger .ic { color: var(--danger); }
  .item.danger.active .ic { background: var(--danger); color: #fff; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-weight: 550; }
  .s { color: var(--muted); font-weight: 400; margin-left: var(--sp-2); }
  .keys { display: inline-flex; gap: 3px; }
  .none { padding: var(--sp-8) var(--sp-4); text-align: center; color: var(--text-2); }
  .none span { color: var(--muted); font-size: var(--fs-sm); }
  .foot { display: flex; gap: var(--sp-4); padding: var(--sp-2) var(--sp-4); border-top: 1px solid var(--border); color: var(--muted); font-size: var(--fs-xs); background: var(--surface-2); }
  .foot span { display: inline-flex; gap: 3px; align-items: center; }
</style>
