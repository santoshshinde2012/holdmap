<script lang="ts">
  // Recently stopped services with one-click restart in their original directory.
  import Icon from "./Icon.svelte";
  import type { HistoryEntry } from "../lib/types";
  import { tildify } from "../lib/format";

  let {
    items,
    onrestart,
    onclear,
    onclose,
    oncopy,
  }: {
    items: HistoryEntry[];
    onrestart: (h: HistoryEntry) => void;
    onclear: () => void;
    onclose: () => void;
    oncopy: (text: string, what: string) => void;
  } = $props();

  let el: HTMLDivElement | undefined = $state();
  $effect(() => el?.focus());
  const when = (ms: number) => {
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400) return `${Math.round(s / 3600)} h ago`;
    return new Date(ms).toLocaleDateString();
  };
  const cmd = (h: HistoryEntry) => `${h.cwd ? `cd ${JSON.stringify(h.cwd)} && ` : ""}${h.command.join(" ")}`;
</script>

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Recently stopped" tabindex="-1" bind:this={el} onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === "Escape" && onclose()}>
    <header>
      <h2><Icon name="history" size={16} />Recently stopped</h2>
      {#if items.length}<button class="btn ghost sm" onclick={onclear}>Clear</button>{/if}
      <button class="icon-btn" aria-label="Close" onclick={onclose}><Icon name="x" size={15} /></button>
    </header>
    {#if !items.length}
      <p class="empty">Nothing yet. Services you stop with portwise show up here so you can bring them back with one click.</p>
    {:else}
      <ul>
        {#each items as h (h.at_ms + ":" + h.pid)}
          <li>
            <span class="port mono">:{h.port}</span>
            <div class="what">
              <div class="t">{h.project ?? h.label}{#if h.framework}<span class="fw"> · {h.framework}</span>{/if}<span class="when">{when(h.at_ms)}</span></div>
              <div class="c mono" title={cmd(h)}>{h.cwd ? tildify(h.cwd) + " $ " : "$ "}{h.command.join(" ")}</div>
            </div>
            <button class="icon-btn" title="Copy command" aria-label="Copy command" onclick={() => oncopy(cmd(h), "Command")}><Icon name="copy" size={13} /></button>
            <button class="btn sm" disabled={!h.command.length} onclick={() => onrestart(h)}><Icon name="play" size={10} />Restart</button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: var(--backdrop); backdrop-filter: blur(3px); display: grid; place-items: center; z-index: 50; }
  .dialog { width: min(680px, calc(100vw - 32px)); max-height: 80vh; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border); border-radius: 16px; box-shadow: var(--shadow-lg); padding: 18px 20px; outline: none; }
  @media (prefers-reduced-motion: no-preference) { .dialog { animation: pop 160ms var(--ease); } }
  @keyframes pop { from { opacity: 0; transform: translateY(6px) scale(0.98); } }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { margin: 0; flex: 1; font-size: var(--fs-lg); display: inline-flex; gap: 8px; align-items: center; }
  .empty { color: var(--muted); margin: 8px 0 4px; }
  ul { list-style: none; margin: 0; padding: 0; overflow-y: auto; display: grid; gap: 2px; }
  li { display: flex; align-items: center; gap: 12px; padding: 8px 6px; border-radius: 10px; }
  li:hover { background: var(--surface-2); }
  .port { font-weight: 700; font-size: 14px; min-width: 58px; color: var(--accent); }
  .what { flex: 1; min-width: 0; }
  .t { font-weight: 600; }
  .fw { color: var(--muted); font-weight: 500; }
  .when { margin-left: 8px; font-size: var(--fs-xs); color: var(--faint); font-weight: 500; }
  .c { font-size: 11px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
