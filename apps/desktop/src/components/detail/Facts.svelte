<script lang="ts" module>
  export interface Fact { label: string; value: string | null | undefined; display?: string; mono?: boolean; copy?: boolean; muted?: string; wrap?: boolean }
</script>

<script lang="ts">
  // Two-column label/value list; values truncate with a tooltip and can be copied.
  import CopyValue from "../ui/CopyValue.svelte";
  let { facts, oncopy }: { facts: Fact[]; oncopy: (text: string, what: string) => void } = $props();
  const shown = $derived(facts.filter((f) => f.value !== null && f.value !== undefined && f.value !== ""));
</script>

<dl>
  {#each shown as f (f.label)}
    <dt>{f.label}</dt>
    <dd>
      {#if f.copy}
        <CopyValue value={f.value!} display={f.display} mono={f.mono} what={f.label} {oncopy} wrap={f.wrap} />
      {:else}
        <span class="v selectable" class:mono={f.mono}>{f.display ?? f.value}</span>
      {/if}
      {#if f.muted}<span class="m">{f.muted}</span>{/if}
    </dd>
  {/each}
</dl>

<style>
  dl { display: grid; grid-template-columns: minmax(0, 104px) minmax(0, 1fr); margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; min-width: 0; }
  dt, dd { padding: 7px 12px; min-height: 34px; display: flex; align-items: center; border-top: 1px solid var(--border); }
  dt:first-of-type, dt:first-of-type + dd { border-top: 0; }
  dt { color: var(--muted); padding-right: 6px; overflow-wrap: anywhere; }
  dd { margin: 0; min-width: 0; gap: 2px 8px; color: var(--text); flex-wrap: wrap; }
  .v { min-width: 0; overflow-wrap: anywhere; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); line-height: var(--lh-mono); }
  .m { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  dd > :global(.cv) { flex: 1; min-width: 0; }
  @container (max-width: 300px) { dl { grid-template-columns: minmax(0, 88px) minmax(0, 1fr); } dt, dd { padding-left: 8px; padding-right: 8px; } }
</style>
