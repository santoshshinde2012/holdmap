<script lang="ts">
  import Icon from "./Icon.svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Kbd from "./ui/Kbd.svelte";
  import { groupResults, rank, type Command } from "../lib/palette";

  let { commands, onclose }: { commands: Command[]; onclose: () => void } = $props();

  let query = $state("");
  let active = $state(0);
  let listEl: HTMLDivElement | undefined = $state();

  const results = $derived(rank(query, commands, 50));
  const groups = $derived(groupResults(query, results));
  const flat = $derived(groups.flatMap((g) => g.items));

  $effect(() => { void query; active = 0; });

  function run(c: Command) {
    onclose();
    queueMicrotask(c.run);
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) { active = Math.min(flat.length - 1, active + 1); scroll(); e.preventDefault(); }
    else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) { active = Math.max(0, active - 1); scroll(); e.preventDefault(); }
    else if (e.key === "Home" && e.ctrlKey) { active = 0; scroll(); e.preventDefault(); }
    else if (e.key === "End" && e.ctrlKey) { active = flat.length - 1; scroll(); e.preventDefault(); }
    else if (e.key === "Enter") { const c = flat[active]; if (c) run(c); e.preventDefault(); }
  }
  function scroll() {
    requestAnimationFrame(() => listEl?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" }));
  }
</script>

<Dialog placement="top" bare label="Command palette" {onclose} initialFocus="input">
  <div class="search">
    <Icon name="search" size={16} />
    <input
      bind:value={query}
      placeholder="Search commands, ports, projects…"
      spellcheck="false"
      autocomplete="off"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-autocomplete="list"
      aria-activedescendant={flat[active] ? `cmd-${flat[active].id}` : undefined}
      onkeydown={key}
    />
    {#if query}<button type="button" class="clear" aria-label="Clear" onclick={() => (query = "")}><Icon name="x" size={12} /></button>{/if}
    <Kbd keys="Esc" size="sm" />
  </div>
  <div class="list" id="palette-list" role="listbox" aria-label="Commands" bind:this={listEl}>
    {#if !flat.length}
      <div class="none"><b>No commands match “{query}”</b><span>Try a port number like <code>3000</code>, a project name or “stop”.</span></div>
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
          onpointermove={() => (active = i)}
          onpointerdown={(e) => e.preventDefault()}
          onclick={() => run(c)}
          onkeydown={() => {}}
        >
          <span class="ic"><Icon name={c.icon ?? "arrow"} size={14} /></span>
          <span class="t"><span class="tt">{c.title}</span>{#if c.subtitle}<span class="s">{c.subtitle}</span>{/if}</span>
          {#if c.shortcut}<Kbd keys={c.shortcut} size="sm" />{:else if i === active}<span class="enter"><Kbd keys="↵" size="sm" /></span>{/if}
        </div>
      {/each}
    {/each}
  </div>
  <div class="foot"><span><Kbd keys={["↑", "↓"]} size="sm" />navigate</span><span><Kbd keys="↵" size="sm" />run</span><span><Kbd keys="Esc" size="sm" />close</span><span class="n">{flat.length} result{flat.length === 1 ? "" : "s"}</span></div>
</Dialog>

<style>
  .search { display: flex; align-items: center; gap: 10px; height: 52px; padding: 0 14px 0 16px; border-bottom: 1px solid var(--border); color: var(--muted); flex: none; }
  .search input { flex: 1; min-width: 0; height: 100%; border: 0; outline: 0; background: transparent; font: inherit; font-size: var(--fs-heading); font-weight: var(--fw-regular); letter-spacing: var(--ls-heading); line-height: var(--lh-heading); color: var(--text); user-select: text; }
  .search input::placeholder { color: var(--faint); }
  .clear { width: 22px; height: 22px; display: grid; place-items: center; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--muted); cursor: pointer; }
  .clear:hover { background: var(--surface-3); color: var(--text); }
  .list { overflow-y: auto; padding: 6px; flex: 1; min-height: 0; scroll-padding: 6px; }
  .gh { font-size: var(--fs-label); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); color: var(--muted); font-weight: var(--fw-medium); padding: 10px 10px 4px; }
  .item { display: flex; align-items: center; gap: 10px; height: 38px; padding: 0 8px; border-radius: var(--r-md); cursor: default; }
  .item.active { background: var(--row-selected); }
  .ic { width: 24px; height: 24px; border-radius: var(--r-sm); display: grid; place-items: center; color: var(--muted); flex: none; border: 1px solid var(--border); background: var(--surface); }
  .item.active .ic { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  /* Destructive commands stay quiet in a list of them and turn red only when highlighted. */
  .item.danger.active .ic { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 30%, transparent); background: var(--danger-soft); }
  .t { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; white-space: nowrap; overflow: hidden; }
  .tt { font-weight: var(--fw-regular); color: var(--text); overflow: hidden; text-overflow: ellipsis; }
  .s { color: var(--muted); font-size: var(--fs-body); line-height: var(--lh-body); overflow: hidden; text-overflow: ellipsis; }
  .none { display: grid; gap: 4px; padding: var(--sp-8) var(--sp-4); text-align: center; color: var(--text-2); }
  .none span { color: var(--muted); font-size: var(--fs-body); line-height: var(--lh-body); }
  .foot { display: flex; align-items: center; gap: 14px; padding: 8px 14px; border-top: 1px solid var(--border); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); background: var(--surface-2); flex: none; }
  .foot span { display: inline-flex; gap: 6px; align-items: center; }
  .foot .n { margin-left: auto; font-variant-numeric: tabular-nums; }
</style>
