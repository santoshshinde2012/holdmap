<script lang="ts">
  // Recently stopped services with one-click restart in their original directory.
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import IconButton from "./ui/IconButton.svelte";
  import Icon from "./Icon.svelte";
  import type { HistoryEntry } from "../lib/types";
  import { tildify } from "../lib/format";
  import { tooltip } from "../lib/tooltip";
  import { commandForShell } from "../lib/shell";
  import Callout from "./ui/Callout.svelte";
  import { fieldId } from "./ui/Field.svelte";

  let { items, platform = "", onrestart, onclear, onclose, oncopy }: { items: HistoryEntry[]; platform?: string; onrestart: (h: HistoryEntry) => Promise<void> | void; onclear: () => Promise<void> | void; onclose: () => void; oncopy: (text: string, what: string) => void } = $props();

  let restarting = $state<string | null>(null);
  let confirmClear = $state(false);
  let clearing = $state(false);
  let clearError = $state<string | null>(null);
  const id = fieldId("history");
  const rows = $derived.by(() => {
    const occurrences = new Map<string, number>();
    return items.map((entry) => {
      const identity = `${entry.at_ms}:${entry.pid}:${entry.port}:${entry.protocol}`;
      const occurrence = occurrences.get(identity) ?? 0;
      occurrences.set(identity, occurrence + 1);
      return { entry, key: `${identity}:${occurrence}` };
    });
  });
  const isWindowsCmd = (h: HistoryEntry) => (platform === "windows" || ((platform === "" || platform === "browser") && typeof navigator !== "undefined" && /Win/.test(navigator.platform))) && /(?:^|[\\/])cmd(?:\.exe)?$/i.test(h.command[0] ?? "");
  const canRestart = (h: HistoryEntry) => !!h.command[0] && (isWindowsCmd(h) ? h.restartable === true : h.restartable !== false);
  const canCopy = (h: HistoryEntry) => !!h.command[0] && (isWindowsCmd(h) ? h.copyable === true : h.copyable !== false);
  const reasonId = (key: string, action: string) => `${id}-${key}-${action}`;
  const restartReason = (h: HistoryEntry) => isWindowsCmd(h) ? "Original Windows shell syntax was not recorded. Start the service normally, then stop it again with holdmap." : "No restart command was recorded. Start the service using its normal launcher.";
  const copyReason = (h: HistoryEntry) => isWindowsCmd(h) ? "Copy unavailable: parsed Windows shell arguments cannot reproduce the original command." : "No copyable shell command was recorded.";
  const when = (ms: number) => {
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    return new Date(ms).toLocaleDateString();
  };
  const cmd = (h: HistoryEntry) => commandForShell(h.command, h.cwd, platform);
  function copy(h: HistoryEntry) { if (canCopy(h)) oncopy(cmd(h), "Command"); }
  async function restart(h: HistoryEntry, key: string) { if (!canRestart(h)) return; restarting = key; try { await onrestart(h); } finally { restarting = null; } }
  async function clear() {
    if (clearing) return;
    clearing = true; clearError = null;
    try { await onclear(); confirmClear = false; } catch (error) { clearError = String(error); } finally { clearing = false; }
  }
</script>

<Dialog title="Recently stopped" description="Bring a service back in the directory it ran from." icon="history" size="lg" {onclose} initialFocus="self">
  {#if !items.length}
    <div class="empty"><Icon name="history" size={22} /><b>Nothing yet</b><span>Services you stop with holdmap show up here so you can restart them with one click.</span></div>
  {:else}
    <ul>
      {#each rows as row (row.key)}
        {@const h = row.entry}
        <li>
          <span class="port">:{h.port}</span>
          <div class="what">
            <div class="t"><span class="name">{h.project ?? h.label}</span>{#if h.framework}<span class="fw">{h.framework}</span>{/if}<span class="when">{when(h.at_ms)}</span></div>
            <div class="c" use:tooltip={{ text: canCopy(h) ? cmd(h) : h.command.join(" "), mono: true, onlyIfTruncated: true }}>{h.cwd ? tildify(h.cwd) + " $ " : "$ "}{h.command.join(" ")}</div>
            {#if !canRestart(h)}<p class="unavailable" id={reasonId(row.key, "restart")}>{restartReason(h)}</p>{/if}
            {#if !canCopy(h)}<p class="unavailable" id={reasonId(row.key, "copy")}>{copyReason(h)}</p>{/if}
          </div>
          <IconButton icon="copy" label="Copy command" size="sm" disabled={!canCopy(h)} aria-describedby={!canCopy(h) ? reasonId(row.key, "copy") : undefined} onclick={() => copy(h)} />
          <Button size="sm" icon="play" disabled={!canRestart(h)} aria-describedby={!canRestart(h) ? reasonId(row.key, "restart") : undefined} loading={restarting === row.key} onclick={() => restart(h, row.key)}>Restart</Button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if clearError}<Callout tone="danger" size="sm">{clearError}</Callout>{/if}
  {#snippet footer()}
    {#if items.length}
      {#if confirmClear}
        <span class="ask">Clear all {items.length} entries?</span>
        <Button variant="ghost" disabled={clearing} onclick={() => (confirmClear = false)}>Keep</Button>
        <Button variant="danger" loading={clearing} loadingText="Clearing…" onclick={clear}>Clear history</Button>
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
  .empty span { max-width: 340px; font-size: var(--fs-body); line-height: var(--lh-body); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  li { display: flex; align-items: center; gap: 12px; padding: 10px 10px 10px 14px; }
  li + li { border-top: 1px solid var(--border); }
  li:hover { background: var(--row-hover); }
  .port { font-family: var(--mono); font-weight: var(--fw-semibold); font-size: var(--fs-mono); line-height: var(--lh-mono); min-width: 56px; color: var(--accent); font-variant-numeric: tabular-nums; }
  .what { flex: 1; min-width: 0; }
  .t { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .name { font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fw { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .when { margin-left: auto; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--faint); white-space: nowrap; }
  .c { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px; }
  .unavailable { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin: 4px 0 0; }
  .sp { flex: 1; }
  .ask { margin-right: auto; font-size: var(--fs-body); line-height: var(--lh-body); color: var(--text-2); font-weight: var(--fw-medium); }
</style>
