<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Kbd from "./ui/Kbd.svelte";
  let { onclose, mod = "Ctrl" }: { onclose: () => void; mod?: string } = $props();
  const groups = $derived<[string, [string[], string][]][]>([
    ["Navigate", [[[mod, "K"], "Command palette"], [["/"], "Search"], [["↑", "↓"], "Move selection (or J / K)"], [["↵"], "Open details"], [["Esc"], "Clear search · close"]]],
    ["Act", [[["⌫"], "Stop selected (graceful)"], [["⇧", "⌫"], "Force kill selected"], [["O"], "Open in browser"], [["C"], "Copy URL"], [["P"], "Pin / unpin"], [["⇧", "P"], "Pin with a label…"], [["S"], "Stop the service's cluster"], [["R"], "Refresh now"]]],
    ["Filter", [[["A"], "Listening ↔ all sockets"], [["T"], "Protocol: any → TCP → UDP"], [["D"], "Dev servers only"], [["M"], "Mine only"], [["E"], "Network-exposed only"]]],
    ["View", [[["G"], "List ↔ graph"], [["H"], "Recently stopped"], [[mod, ","], "Settings"], [["⇧", "L"], "Cycle theme"], [["?"], "This help"]]],
  ]);
</script>

<Dialog title="Keyboard shortcuts" description="Everything in portwise works without a mouse." icon="keyboard" size="lg" {onclose} initialFocus="self">
  <div class="grid">
    {#each groups as [name, items]}
      <section>
        <h3>{name}</h3>
        {#each items as [keys, label]}<div class="item"><span>{label}</span><Kbd {keys} /></div>{/each}
      </section>
    {/each}
  </div>
  <p class="hint">Search understands <code>:3000</code>, <code>3000-3999</code>, <code>proto:udp</code>, <code>pid:123</code> and free text like <code>next</code> or a branch name.</p>
</Dialog>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: var(--sp-2) var(--sp-8); }
  h3 { margin: var(--sp-3) 0 var(--sp-1); font-size: var(--fs-label); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); color: var(--muted); font-weight: var(--fw-medium); }
  .item { display: flex; justify-content: space-between; align-items: center; gap: var(--sp-3); min-height: 30px; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); border-bottom: 1px solid var(--border); }
  .hint { color: var(--muted); margin: var(--sp-5) 0 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
</style>
