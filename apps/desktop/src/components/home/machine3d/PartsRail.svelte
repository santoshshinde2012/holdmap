<script lang="ts">
  // A button for every part: the agents in ring order, and the selected agent's parts by layer.
  // It is also the accessible outline of the canvas: everything drawn is reachable here, and
  // ↑/↓ move between buttons.
  import Icon from "../../Icon.svelte";
  import { agentSummary, LAYER_LABEL, PART_KIND_LABEL, type Layer, type MachineScene, type Part } from "../../../lib/machine3d/scene";

  let { scene, colors, focus, selected, onagent, onpart, onhover }: {
    scene: MachineScene;
    colors: ReadonlyMap<string, string>;
    /** The agent whose parts are listed (selected, or the owner of the selected part). */
    focus: string | null;
    /** Selected part or agent id. */
    selected: string | null;
    onagent: (id: string) => void;
    onpart: (part: Part, el: HTMLElement) => void;
    onhover: (id: string | null) => void;
  } = $props();

  const LAYERS: Layer[] = ["runtime", "board", "config"];
  const ICON: Record<Part["kind"], string> = { agent: "bot", dormant: "bot", project: "folder", port: "plug", remote: "globe", tool: "terminal", mcp: "server", skill: "sparkles", instructions: "list", more: "more", call: "activity" };
  const own = $derived(focus ? scene.parts.filter((p) => p.owner === focus && p.kind !== "agent" && p.kind !== "dormant") : []);
  const groups = $derived(LAYERS.map((layer) => ({ layer, parts: own.filter((p) => p.layer === layer) })).filter((g) => g.parts.length));
  let nav = $state<HTMLElement>();

  function key(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
    const buttons = [...(nav?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 : Math.min(buttons.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)));
    buttons[next]?.focus();
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav class="rail" aria-label="Machine parts" bind:this={nav} onkeydown={key}>
  <h3>Agents</h3>
  <ul>
    {#each scene.agents as a (a.id)}
      <li>
        <!-- The visible text is the accessible name (WCAG 2.5.3); extra context is screen-reader text. -->
        <button type="button" class="agent" class:on={focus === a.id} class:dormant={a.kind === "dormant"} class:warn={!!a.warn} aria-pressed={focus === a.id}
          style="--agent: {colors.get(a.id) ?? 'var(--muted)'}"
          onclick={() => onagent(a.id)} onmouseenter={() => onhover(a.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(a.id)} onblur={() => onhover(null)}>
          <i aria-hidden="true"></i>
          <span class="t"><b>{a.label}</b>{" "}<small>{a.kind === "dormant" ? "Not running" : agentSummary(scene, a.id)}</small></span>
          {#if a.warn}<span class="dot" aria-hidden="true"></span><span class="sr-only">, needs you</span>{/if}
        </button>
        {#if focus === a.id && groups.length}
          <div class="parts">
            {#each groups as g (g.layer)}
              <h4>{LAYER_LABEL[g.layer]}</h4>
              <ul>
                {#each g.parts as p (p.id)}
                  <li>
                    <button type="button" class="part" class:on={selected === p.id} class:warn={!!p.warn} class:ghost={p.evidence === "configured" && !p.live} aria-pressed={selected === p.id}
                      aria-haspopup={p.kind === "port" ? "menu" : undefined} onclick={(e) => onpart(p, e.currentTarget)}>
                      <Icon name={ICON[p.kind]} size={12} />
                      <!-- Separate space nodes keep words apart in the computed name. -->
                      <span class="t"><b><span class="sr-only">{PART_KIND_LABEL[p.kind]}</span>{" "}{p.label}</b>{#if p.sub}{" "}<small>{p.sub}</small>{/if}</span>
                      {#if p.live && p.kind === "mcp"}{" "}<span class="run">running</span>{/if}
                      {#if p.warn}<span class="sr-only">, needs attention</span>{/if}
                    </button>
                  </li>
                {/each}
              </ul>
            {/each}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
  {#if scene.omittedAgents}<p class="omitted">+{scene.omittedAgents} more on Agents</p>{/if}
</nav>

<style>
  .rail { min-height: 0; overflow-y: auto; padding: var(--sp-2); border-right: 1px solid var(--border); background: var(--surface); }
  h3, h4 { margin: var(--sp-2) var(--sp-2) var(--sp-1); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-semibold); color: var(--muted); }
  h4 { margin: var(--sp-2) 0 2px var(--sp-1); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  button { position: relative; display: flex; align-items: center; gap: 8px; width: 100%; min-height: 32px; padding: 5px 8px; border: 1px solid transparent; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  button:hover { background: var(--row-hover); }
  button.on { background: var(--row-selected); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: -1px; }
  .t { display: grid; min-width: 0; flex: 1; }
  b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .agent i { flex: none; width: 10px; height: 10px; border-radius: 3px; background: var(--agent); }
  .agent.dormant i { background: transparent; border: 1.5px dashed var(--muted); }
  .agent.dormant b { color: var(--text-2); font-weight: var(--fw-medium); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--danger); flex: none; }
  .parts { margin: 2px 0 6px 12px; padding-left: 8px; border-left: 1px solid var(--border); }
  .part { min-height: 28px; padding: 3px 6px; gap: 7px; color: var(--text-2); }
  .part b { font-weight: var(--fw-medium); color: var(--text); }
  .part.ghost :global(svg) { color: var(--tone-blue); }
  .part.warn :global(svg), .part.warn small { color: var(--warn); }
  .run { flex: none; padding: 0 6px; border-radius: var(--r-full); background: var(--ok-soft); color: var(--ok); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .omitted { margin: var(--sp-2); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
</style>
