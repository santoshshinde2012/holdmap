<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Kbd from "./ui/Kbd.svelte";
  let { onclose, mod = "Ctrl", singleKeyShortcuts = true }: { onclose: () => void; mod?: string; singleKeyShortcuts?: boolean } = $props();
  const allGroups = $derived<[string, [string[], string][]][]>([
    ["Navigate", [[[mod, "K"], "Command palette"], [[mod, "F"], "Search"], [["/"], "Search"], [["↑", "↓"], singleKeyShortcuts ? "Move selection (or J / K)" : "Move selection"], [["←", "→"], "Collapse · expand section"], [["Home", "End"], "First · last port"], [["↵"], "Open details"], [["Esc"], "Clear search · close"]]],
    ["Act", [[["⌫"], "Stop selected (graceful)"], [["⇧", "⌫"], "Force kill selected"], [["O"], "Open in browser"], [["C"], "Copy URL"], [["P"], "Pin / unpin"], [["⇧", "P"], "Pin with a label…"], [["S"], "Stop the service's cluster"], [["R"], "Refresh now"], [["⇧", "S"], "Take snapshot (PNG)"], [["⇧", "V"], "Record screen (MP4 · GIF)"]]],
    ["List filters", [[["A"], "Listening ↔ all sockets"], [["T"], "Protocol: any → TCP → UDP"], [["D"], "Dev servers only"], [["M"], "Mine only"], [["E"], "Network-exposed only"]]],
    ["View", [[["G"], "List ↔ graph"], [["V"], "Home: cards ↔ map"], [["⇧", "A"], "Agents, tools & apps (reveal · stop)"], [["B"], "Hide · show details pane"], [["H"], "Recently stopped"], [[mod, ","], "Settings"], [["⇧", "L"], "Cycle theme"], [["?"], "This help"]]],
  ]);
  const groups = $derived(allGroups.map(([name, items]): [string, [string[], string][]] => [name, items.filter(([keys]) => singleKeyShortcuts || keys.some((key) => key === mod || ["↑", "↓", "←", "→", "Home", "End", "↵", "Esc", "⌫"].includes(key)))]));
</script>

<Dialog title="Keyboard shortcuts" description="Everything in holdmap works without a mouse." icon="keyboard" size="lg" {onclose} initialFocus="self">
  {#if !singleKeyShortcuts}<p class="hint">Single-key shortcuts are off. Enable them in Settings → General. Cmd/Ctrl shortcuts and arrow navigation remain available.</p>{/if}
  <div class="grid">
    {#each groups as [name, items]}
      {#if items.length}
      <section>
        <h3>{name}</h3>
        {#each items as [keys, label]}<div class="item"><span>{label}</span><Kbd {keys} /></div>{/each}
      </section>
      {/if}
    {/each}
  </div>
  <p class="hint">Port filters apply in List. Selected-port actions apply in List and in open Graph details. Shortcuts stay out of text fields and native controls.</p>
  <p class="hint">List search understands <code>:3000</code>, <code>3000-3999</code>, <code>proto:udp</code>, <code>pid:123</code> and free text like <code>next</code> or a branch name.</p>
  <p class="hint">Graph search matches service names, projects, ports and PIDs. Use <code>:3000</code> or <code>port:3000</code> for an exact port, and <code>pid:123</code> for an exact PID. Results keep directly connected neighbors for context.</p>
</Dialog>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: var(--sp-2) var(--sp-8); }
  h3 { margin: var(--sp-3) 0 var(--sp-1); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-weight: var(--fw-medium); }
  .item { display: flex; justify-content: space-between; align-items: center; gap: var(--sp-3); min-height: 30px; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); border-bottom: 1px solid var(--border); }
  .hint { color: var(--muted); margin: var(--sp-5) 0 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
</style>
