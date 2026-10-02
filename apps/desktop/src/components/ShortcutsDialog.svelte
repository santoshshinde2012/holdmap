<script lang="ts">
  let { onclose }: { onclose: () => void } = $props();
  const mod = typeof navigator !== "undefined" && /Mac/.test(navigator.platform) ? "⌘" : "Ctrl";
  const groups: [string, [string[], string][]][] = [
    ["Navigate", [[["/"], "Search"], [[mod, "K"], "Search"], [["↑", "↓"], "Move selection (or J / K)"], [["Esc"], "Clear search · close"]]],
    ["Act", [[["⌫"], "Stop selected (graceful)"], [["⇧", "⌫"], "Force kill selected"], [["O"], "Open in browser"], [["C"], "Copy URL"], [["R"], "Refresh now"]]],
    ["Filter", [[["A"], "Listening ↔ all sockets"], [["T"], "Protocol: any → TCP → UDP"], [["D"], "Dev servers only"], [["M"], "Mine only"], [["E"], "Network-exposed only"]]],
    ["App", [[["⇧", "L"], "Cycle theme (system/light/dark)"], [["?"], "Show this help"]]],
  ];
  let el: HTMLDivElement | undefined = $state();
  $effect(() => el?.focus());
</script>

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" tabindex="-1" bind:this={el} onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <h2>Keyboard shortcuts</h2>
    <div class="grid">
      {#each groups as [name, items]}
        <section>
          <h4>{name}</h4>
          {#each items as [keys, label]}
            <div class="item"><span>{label}</span><span class="keys">{#each keys as k}<kbd>{k}</kbd>{/each}</span></div>
          {/each}
        </section>
      {/each}
    </div>
    <p class="hint">Search understands <code>:3000</code>, <code>3000-3999</code>, <code>proto:udp</code>, <code>pid:123</code> and free text like <code>next</code> or a branch name.</p>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(8 10 16 / 0.45); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 50; }
  .dialog { width: min(640px, calc(100vw - 32px)); background: var(--surface); border: 1px solid var(--border); border-radius: 16px; box-shadow: var(--shadow-lg); padding: 22px; outline: none; }
  h2 { margin: 0 0 14px; font-size: 16px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px 28px; }
  h4 { margin: 8px 0 6px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.07em; color: var(--muted); }
  .item { display: flex; justify-content: space-between; align-items: center; padding: 4px 0; color: var(--text-2); }
  .keys { display: inline-flex; gap: 3px; }
  .hint { color: var(--muted); margin: 16px 0 0; font-size: 12px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
</style>
