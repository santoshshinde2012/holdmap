<script lang="ts">
  // The selected part: what it is, how sure holdmap is, the facts behind it, and the same actions
  // as the cards and the map. Port actions open the port menu; Stop… always shows the plan first.
  import Icon from "../../Icon.svelte";
  import EvidenceChip from "../../ui/EvidenceChip.svelte";
  import { partChip } from "../../../lib/machine3d/describe";
  import { PART_KIND_LABEL, type Part } from "../../../lib/machine3d/scene";

  let { part, flat = false, onclose, onopenagent, onport }: {
    part: Part;
    /** Inside Home's Details panel rather than floating over the canvas. */
    flat?: boolean;
    onclose: () => void;
    onopenagent: (id: string) => void;
    onport: (entryId: string, anchor: HTMLElement) => void;
  } = $props();

  const chip = $derived(partChip(part));
  let portBtn = $state<HTMLButtonElement>();
</script>

<section class="inspector" class:flat aria-label="{PART_KIND_LABEL[part.kind]} {part.label}">
  <header>
    <div class="h">
      <small>{PART_KIND_LABEL[part.kind]}</small>
      <h3>{part.label}</h3>
    </div>
    <button type="button" class="x" aria-label="Close details" title="Close (Esc)" onclick={onclose}><Icon name="x" size={14} /></button>
  </header>
  <div class="chips"><EvidenceChip {...chip} /></div>
  {#if part.warn}<p class="warn"><Icon name="alert" size={13} />{part.warn}</p>{/if}
  {#if part.facts.length}
    <dl>
      {#each part.facts as [k, v], i (i)}<div><dt>{k}</dt><dd>{v}</dd></div>{/each}
    </dl>
  {/if}
  <div class="acts">
    {#if part.target?.view === "list"}
      {@const entryId = part.target.entryId}
      <button type="button" class="act primary" bind:this={portBtn} aria-haspopup="menu" onclick={() => onport(entryId, portBtn!)}>Port actions…</button>
    {/if}
    {#if part.kind !== "dormant" && part.kind !== "call"}
      <button type="button" class="act" onclick={() => onopenagent(part.owner)}><Icon name="arrow" size={12} />Open in Agents</button>
    {/if}
  </div>
</section>

<style>
  .inspector { pointer-events: auto; width: 300px; max-height: 100%; overflow-y: auto; padding: 10px 12px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 94%, transparent); box-shadow: var(--shadow-md); backdrop-filter: blur(6px); }
  .inspector.flat { width: auto; max-height: none; overflow: visible; margin: 0 6px 6px; padding: 10px 10px 12px; border: 0; border-top: 1px solid var(--border); border-radius: 0; background: transparent; box-shadow: none; backdrop-filter: none; }
  header { display: flex; align-items: flex-start; gap: 8px; }
  .h { flex: 1; min-width: 0; }
  small { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); font-weight: var(--fw-semibold); }
  h3 { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; }
  .x { display: grid; place-items: center; width: 26px; height: 26px; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); cursor: pointer; flex: none; }
  .x:hover { background: var(--row-hover); color: var(--text); }
  .x:focus-visible, .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .chips { margin: 6px 0 4px; }
  .warn { display: flex; gap: 6px; align-items: flex-start; margin: 6px 0; padding: 6px 8px; border-radius: var(--r-md); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .warn :global(svg) { flex: none; margin-top: 2px; }
  dl { margin: 6px 0 0; display: grid; gap: 4px; }
  dl div { display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: 8px; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text); overflow-wrap: anywhere; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .act { display: inline-flex; align-items: center; gap: 5px; height: 28px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body-sm); font-weight: var(--fw-medium); cursor: pointer; }
  .act:hover { background: var(--row-hover); }
  .act.primary { color: var(--accent); }
</style>
