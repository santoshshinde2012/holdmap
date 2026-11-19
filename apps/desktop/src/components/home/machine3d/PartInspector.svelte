<script lang="ts">
  // The selected part: what it is, how sure holdmap is, the facts behind it, and the same actions
  // as the cards and the map. Port actions open the port menu; Stop… always shows the plan first.
  import type { Snippet } from "svelte";
  import Icon from "../../Icon.svelte";
  import EvidenceChip from "../../ui/EvidenceChip.svelte";
  import { partChip } from "../../../lib/machine3d/describe";
  import { PART_KIND_LABEL, type Part } from "../../../lib/machine3d/scene";
  import { AGENT_DOORS, partDestination, type AgentJump } from "../../../lib/agent-jump";

  let { part, flat = false, onclose, onopenagent, onport, children }: {
    part: Part;
    /** Inside Home's Details panel rather than floating over the canvas. */
    flat?: boolean;
    onclose: () => void;
    onopenagent: (id: string, jump?: AgentJump, capabilityId?: string) => void;
    onport: (entryId: string, anchor: HTMLElement) => void;
    /** Sits between the next step and the fact list, so a long dossier doesn't cover it. */
    children?: Snippet;
  } = $props();

  const chip = $derived(partChip(part));
  const go = $derived(partDestination(part.kind, part.variant));
  const others = $derived(part.kind === "call" ? [] : AGENT_DOORS.filter((door) => door.jump !== go.jump));
  let portBtn = $state<HTMLButtonElement>();

  function open(jump: AgentJump) {
    if (jump === "calls") onopenagent("", "calls");
    else if (jump === "capabilities" && part.capabilityId) onopenagent(part.owner, jump, part.capabilityId);
    else onopenagent(part.owner, jump);
  }
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
  <div class="acts" role="group" aria-label={part.kind === "call" ? "Open recorded calls" : "Open this agent"}>
    {#if part.target?.view === "list"}
      {@const entryId = part.target.entryId}
      <button type="button" class="menu" bind:this={portBtn} aria-haspopup="menu" onclick={() => onport(entryId, portBtn!)}>Port actions…<Icon name="chevron-down" size={14} /></button>
    {/if}
    <button type="button" class="go" title={part.kind === "call" ? go.hint : `${go.hint}. Click the part again to open this.`} onclick={() => open(go.jump)}>
      <Icon name={go.icon} size={15} />
      <span class="go-copy"><b>{go.label}</b>{" "}<small>{go.hint}</small></span>
      <Icon name="arrow" size={14} />
    </button>
    {#if others.length}
      <div class="more">
        {#each others as door (door.jump)}
          <button type="button" class="act" title={door.title} onclick={() => open(door.jump)}><Icon name={door.icon} size={12} />{door.label}</button>
        {/each}
      </div>
    {/if}
  </div>
  {@render children?.()}
  {#if part.facts.length}
    <dl>
      {#each part.facts as [k, v], i (i)}<div><dt>{k}</dt><dd>{v}</dd></div>{/each}
    </dl>
  {/if}
</section>

<style>
  .inspector { pointer-events: auto; width: 300px; max-height: 100%; overflow-y: auto; padding: 10px 12px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 94%, transparent); box-shadow: var(--shadow-md); backdrop-filter: blur(6px); }
  .inspector.flat { width: auto; max-height: none; overflow: visible; margin: 0; padding: 10px 10px 4px; border: 0; border-radius: 0; background: transparent; box-shadow: none; backdrop-filter: none; }
  header { display: flex; align-items: flex-start; gap: 8px; }
  .h { flex: 1; min-width: 0; }
  header small { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); font-weight: var(--fw-semibold); }
  h3 { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; }
  .x { display: grid; place-items: center; width: 26px; height: 26px; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); cursor: pointer; flex: none; }
  .x:hover { background: var(--row-hover); color: var(--text); }
  .x:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .chips { margin: 8px 0 0; }
  .warn { display: flex; gap: 6px; align-items: flex-start; margin: 6px 0; padding: 6px 8px; border-radius: var(--r-md); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .warn :global(svg) { flex: none; margin-top: 2px; }
  dl { margin: 6px 0 0; display: grid; gap: 4px; }
  dl div { display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: 8px; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text); overflow-wrap: anywhere; }
  .acts { display: grid; gap: 6px; margin-top: 10px; }
  .menu, .go, .act { font: inherit; cursor: pointer; }
  .menu { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 32px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--text); font-size: var(--fs-body-sm); font-weight: var(--fw-medium); }
  .menu:hover, .act:hover { background: var(--row-hover); }
  .go { display: flex; align-items: center; gap: 8px; width: 100%; min-height: 44px; padding: 6px 10px; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border)); border-radius: var(--r-md); background: var(--accent-softer); color: var(--text); text-align: left; }
  .go:hover { background: color-mix(in srgb, var(--accent) 16%, var(--surface)); }
  .go:active, .menu:active, .act:active { transform: translateY(1px); }
  .go-copy { display: grid; gap: 1px; min-width: 0; flex: 1; }
  .go b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); }
  .go small { color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); letter-spacing: normal; text-transform: none; font-weight: var(--fw-regular); }
  .go :global(svg:last-child) { color: var(--accent); flex: none; }
  .more { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
  .act { display: inline-flex; align-items: center; gap: 5px; min-height: 32px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--surface); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); text-align: left; }
  .act:hover { color: var(--text); border-color: var(--border-strong); background: var(--row-hover); }
  .menu:focus-visible, .go:focus-visible, .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
