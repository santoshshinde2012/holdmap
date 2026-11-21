<script lang="ts">
  // The selected part: what it is, how sure holdmap is, the facts behind it, and the same actions
  // as the cards and the map. Port actions open the port menu; Stop… always shows the plan first.
  import type { Snippet } from "svelte";
  import Icon from "../../Icon.svelte";
  import EvidenceChip from "../../ui/EvidenceChip.svelte";
  import { partChip } from "../../../lib/machine3d/describe";
  import { PART_KIND_LABEL, type Part } from "../../../lib/machine3d/scene";
  import { AGENT_DOORS, partDestination, type AgentJump } from "../../../lib/agent-jump";
  import { prefGetBoolean, prefSet } from "../../../lib/prefs";
  import { tooltip } from "../../../lib/tooltip";

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
  /** A remote group lists hosts. Addresses are the fact, not a short label, so they get their own rows. */
  const hosts = $derived(part.kind === "remote" ? part.facts.filter(([k]) => k !== "Names" && k !== "More") : []);
  const hostNotes = $derived(part.kind === "remote" ? part.facts.filter(([k]) => k === "Names" || k === "More").map(([, v]) => v) : []);
  const factCount = $derived(hosts.length || part.facts.length);
  // The readings and the facts are long. Each folds on its own, and Home remembers that, so the
  // doors stay within reach.
  let readingsOpen = $state(prefGetBoolean("home.details.readings", true));
  let factsOpen = $state(prefGetBoolean("home.details.facts", true));
  $effect(() => prefSet("home.details.readings", readingsOpen ? "1" : "0"));
  $effect(() => prefSet("home.details.facts", factsOpen ? "1" : "0"));
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
    <button type="button" class="go" onclick={() => open(go.jump)}>
      <Icon name={go.icon} size={15} />
      <span class="go-copy"><b>{go.label}</b>{" "}<small>{go.hint}</small></span>
      <Icon name="arrow" size={14} />
    </button>
    {#if others.length}
      <div class="more">
        {#each others as door (door.jump)}
          <button type="button" class="act" use:tooltip={{ text: door.title, placement: "bottom" }} onclick={() => open(door.jump)}><Icon name={door.icon} size={12} />{door.label}</button>
        {/each}
      </div>
    {/if}
  </div>
  {#if children}
    <section class="block">
      <button type="button" class="fold" aria-expanded={readingsOpen} aria-controls="part-readings-{part.id}" onclick={() => (readingsOpen = !readingsOpen)}>
        <span class="chev" aria-hidden="true"><Icon name="chevron-down" size={14} /></span><span>Readings</span>
      </button>
      {#if readingsOpen}<div id="part-readings-{part.id}">{@render children()}</div>{/if}
    </section>
  {/if}
  {#if factCount}
    <section class="block">
      <button type="button" class="fold" aria-expanded={factsOpen} aria-controls="part-facts-{part.id}" onclick={() => (factsOpen = !factsOpen)}>
        <span class="chev" aria-hidden="true"><Icon name="chevron-down" size={14} /></span><span>{hosts.length ? "Hosts" : "Facts"}</span><span class="n">{factCount}</span>
      </button>
      {#if factsOpen}
        <div id="part-facts-{part.id}">
          {#if hosts.length}
            <ul class="hosts" aria-label="Observed hosts">
              {#each hosts as [address, connections], i (i)}
                <li>
                  <span class="addr">{address}</span>
                  <span class="count">{connections}</span>
                </li>
              {/each}
            </ul>
            {#each hostNotes as note, i (i)}<p class="note">{note}</p>{/each}
          {:else}
            <dl>
              {#each part.facts as [k, v], i (i)}<div><dt>{k}</dt><dd>{v}</dd></div>{/each}
            </dl>
          {/if}
        </div>
      {/if}
    </section>
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
  .block { margin-top: 8px; padding-top: 4px; border-top: 1px solid var(--border); }
  .fold { display: flex; align-items: center; gap: 6px; width: 100%; min-height: 28px; padding: 2px 4px; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; cursor: pointer; text-align: left; }
  .fold:hover { background: var(--row-hover); color: var(--text); }
  .fold:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .chev { display: inline-flex; color: var(--muted); transition: transform 160ms ease; }
  .fold[aria-expanded="false"] .chev { transform: rotate(-90deg); }
  @media (prefers-reduced-motion: reduce) { .chev { transition: none; } }
  .n { margin-left: auto; color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; letter-spacing: normal; text-transform: none; }
  dl, .hosts { margin: 4px 0 0; }
  dl { display: grid; gap: 4px; }
  dl div { display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: 8px; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text); overflow-wrap: anywhere; }
  .hosts { list-style: none; padding-left: 0; padding-right: 0; display: grid; gap: 4px; }
  .hosts li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px; align-items: start; padding: 6px 8px; border-radius: var(--r-sm); background: var(--surface-2); }
  .addr { min-width: 0; font-family: var(--mono); font-size: var(--fs-mono); line-height: var(--lh-mono); letter-spacing: var(--ls-mono); color: var(--text); overflow-wrap: anywhere; }
  .count { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; padding-top: 1px; }
  .note { margin: 8px 2px 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
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
