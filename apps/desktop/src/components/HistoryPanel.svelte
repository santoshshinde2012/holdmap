<script lang="ts">
  // Recently stopped services with one-click restart in their original directory.
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import IconButton from "./ui/IconButton.svelte";
  import Icon from "./Icon.svelte";
  import type { HistoryEntry } from "../lib/types";
  import { tildify } from "../lib/format";
  import { tooltip } from "../lib/tooltip";

  let { items, onrestart, onclear, onclose, oncopy }: { items: HistoryEntry[]; onrestart: (h: HistoryEntry) => Promise<void> | void; onclear: () => void; onclose: () => void; oncopy: (text: string, what: string) => void } = $props();

  let restarting = $state<string | null>(null);
  let confirmClear = $state(false);
  const key = (h: HistoryEntry) => `${h.at_ms}:${h.pid}`;
  const when = (ms: number) => {
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    return new Date(ms).toLocaleDateString();
  };
  const cmd = (h: HistoryEntry) => `${h.cwd ? `cd ${JSON.stringify(h.cwd)} && ` : ""}${h.command.join(" ")}`;
  async function restart(h: HistoryEntry) { restarting = key(h); try { await onrestart(h); } finally { restarting = null; } }
</script>

<Dialog title="Recently stopped" description="Bring a service back in the directory it ran from." icon="history" size="lg" {onclose} initialFocus="self">
  {#if !items.length}
    <div class="empty"><Icon name="history" size={22} /><b>Nothing yet</b><span>Services you stop with portwise show up here so you can restart them with one click.</span></div>
  {:else}
    <ul>
      {#each items as h (key(h))}
        <li>
          <span class="port">:{h.port}</span>
          <div class="what">
            <div class="t"><span class="name">{h.project ?? h.label}</span>{#if h.framework}<span class="fw">{h.framework}</span>{/if}<span class="when">{when(h.at_ms)}</span></div>
            <div class="c" use:tooltip={{ text: cmd(h), mono: true, onlyIfTruncated: true }}>{h.cwd ? tildify(h.cwd) + " $ " : "$ "}{h.command.join(" ")}</div>
          </div>
          <IconButton icon="copy" label="Copy command" size="sm" onclick={() => oncopy(cmd(h), "Command")} />
          <Button size="sm" icon="play" disabled={!h.command.length} loading={restarting === key(h)} onclick={() => restart(h)}>Restart</Button>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    {#if items.length}
      {#if confirmClear}
        <span class="ask">Clear all {items.length} entries?</span>
        <Button variant="ghost" onclick={() => (confirmClear = false)}>Keep</Button>
        <Button variant="danger" onclick={() => { confirmClear = false; onclear(); }}>Clear history</Button>
      {:else}
        <Button variant="ghost" icon="x" onclick={() => (confirmClear = true)}>Clear history…</Button>
        <span class="sp"></span>
        <Button onclick={onclose}>Done</Button>
      {/if}
    {:else}
      <Button onclick={onclose}>Done</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .empty { display: grid; justify-items: center; gap: 6px; text-align: center; color: var(--muted); padding: var(--sp-8) var(--sp-6); }
  .empty b { color: var(--text); }
  .empty span { max-width: 340px; font-size: var(--fs-sm); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  li { display: flex; align-items: center; gap: 12px; padding: 10px 10px 10px 14px; }
  li + li { border-top: 1px solid var(--border); }
  li:hover { background: var(--row-hover); }
  .port { font-family: var(--mono); font-weight: 700; font-size: 14px; min-width: 56px; color: var(--accent); font-variant-numeric: tabular-nums; }
  .what { flex: 1; min-width: 0; }
  .t { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .name { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fw { color: var(--muted); font-size: var(--fs-xs); }
  .when { margin-left: auto; font-size: var(--fs-xs); color: var(--faint); white-space: nowrap; }
  .c { font-family: var(--mono); font-size: 11px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px; }
  .sp { flex: 1; }
  .ask { margin-right: auto; font-size: var(--fs-sm); color: var(--text-2); font-weight: 550; }
</style>
