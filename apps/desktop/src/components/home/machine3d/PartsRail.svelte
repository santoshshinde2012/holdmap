<script lang="ts">
  // A button for every part: the agents in ring order, and the selected agent's parts by layer.
  // It is also the accessible outline of the canvas: everything drawn is reachable here, and
  // ↑/↓ move between buttons.
  import Icon from "../../Icon.svelte";
  import { partDestination } from "../../../lib/agent-jump";
  import { agentSummary, LAYER_LABEL, PART_KIND_LABEL, type Layer, type MachineScene, type Part } from "../../../lib/machine3d/scene";

  let { scene, colors, focus, selected, query = "", onquery, collapsed = false, ontoggle, onagent, onpart, onhover }: {
    scene: MachineScene;
    colors: ReadonlyMap<string, string>;
    /** The agent whose parts are listed (selected, or the owner of the selected part). */
    focus: string | null;
    /** Selected part or agent id. */
    selected: string | null;
    /** Filters the list and lights matching parts in the picture. */
    query?: string;
    onquery?: (query: string) => void;
    /** Folded to a strip of agent buttons, so the canvas gets the width. */
    collapsed?: boolean;
    ontoggle?: () => void;
    onagent: (id: string) => void;
    onpart: (part: Part, el: HTMLElement) => void;
    onhover: (id: string | null) => void;
  } = $props();

  const LAYERS: Layer[] = ["runtime", "board", "config"];
  const ICON: Record<Part["kind"], string> = { agent: "bot", dormant: "bot", project: "folder", port: "plug", remote: "globe", tool: "terminal", mcp: "server", skill: "sparkles", instructions: "list", more: "more", call: "activity" };
  const q = $derived(query.trim().toLowerCase());
  const hit = (p: Part) => `${p.label} ${p.sub ?? ""}`.toLowerCase().includes(q);
  const agents = $derived(!q ? scene.agents : scene.agents.filter((a) => a.label.toLowerCase().includes(q) || scene.parts.some((p) => p.owner === a.id && hit(p))));
  const shared = $derived.by(() => {
    const n = new Map<string, number>();
    for (const a of scene.agents) n.set(a.label, (n.get(a.label) ?? 0) + 1);
    return n;
  });
  const pidOf = (a: Part) => a.facts.find(([k]) => k === "PID")?.[1];
  /** Two agents can share a product name. The PID is what tells them apart. */
  const who = (a: Part) => ((shared.get(a.label) ?? 0) > 1 && pidOf(a) ? `${a.label} ${pidOf(a)}` : a.label);
  const about = (a: Part) => {
    const summary = a.kind === "dormant" ? "Not running" : agentSummary(scene, a.id);
    const pid = pidOf(a);
    return (shared.get(a.label) ?? 0) > 1 && pid ? `PID ${pid} · ${summary}` : summary;
  };
  const named = $derived(!!focus && agents.some((a) => a.id === focus && a.label.toLowerCase().includes(q)));
  const own = $derived(focus ? scene.parts.filter((p) => p.owner === focus && p.kind !== "agent" && p.kind !== "dormant" && (!q || named || hit(p))) : []);
  const groups = $derived(LAYERS.map((layer) => ({ layer, parts: own.filter((p) => p.layer === layer) })).filter((g) => g.parts.length));
  let nav = $state<HTMLElement>();

  function key(e: KeyboardEvent) {
    if (e.target instanceof HTMLElement && e.target.closest("input, textarea")) return;
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
    const buttons = [...(nav?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 : Math.min(buttons.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)));
    buttons[next]?.focus();
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav class="rail" class:collapsed aria-label="Machine parts" bind:this={nav} onkeydown={key}>
  <div class="head">
    {#if !collapsed}<h3>Agents</h3>{/if}
    {#if !collapsed}
      <label class="find">
        <span class="sr-only">Find a part</span>
        <input type="search" placeholder="Find a port, skill, process…" value={query} oninput={(e) => onquery?.(e.currentTarget.value)} />
      </label>
    {/if}
    {#if ontoggle}
      <button type="button" class="fold" aria-expanded={!collapsed} title={collapsed ? "Show the parts list" : "Fold the parts list"} onclick={ontoggle}>
        <Icon name="sidebar" size={14} /><span class="sr-only">{collapsed ? "Show the parts list" : "Fold the parts list"}</span>
      </button>
    {/if}
  </div>
  {#if collapsed}
    <!-- Folded: one button per agent, in its colour, named for screen readers and tooltips. -->
    <ul class="dots">
      {#each agents as a (a.id)}
        <li>
          <button type="button" class="dotbtn" class:on={focus === a.id} class:dormant={a.kind === "dormant"} aria-pressed={focus === a.id} title={who(a)}
            style="--agent: {colors.get(a.id) ?? 'var(--muted)'}" onclick={() => onagent(a.id)} onmouseenter={() => onhover(a.id)} onmouseleave={() => onhover(null)}>
            <i aria-hidden="true"></i><span class="sr-only">{who(a)}</span>{#if a.warn}<span class="dot" aria-hidden="true"></span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {:else}
  <ul>
    {#each agents as a (a.id)}
      <li>
        <!-- The visible text is the accessible name (WCAG 2.5.3); extra context is screen-reader text. -->
        <button type="button" class="agent" class:on={focus === a.id} class:dormant={a.kind === "dormant"} class:warn={!!a.warn} aria-pressed={focus === a.id}
          style="--agent: {colors.get(a.id) ?? 'var(--muted)'}"
          title={selected === a.id ? `${partDestination(a.kind, a.variant).label}. Click again.` : undefined}
          onclick={() => onagent(a.id)} onmouseenter={() => onhover(a.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(a.id)} onblur={() => onhover(null)}>
          <i aria-hidden="true"></i>
          <span class="t"><b>{a.label}</b>{" "}<small>{about(a)}</small></span>
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
                      aria-haspopup={p.kind === "port" ? "menu" : undefined}
                      title={selected === p.id ? `${partDestination(p.kind, p.variant).label}. Click again.` : undefined}
                      onclick={(e) => onpart(p, e.currentTarget)}>
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
  {#if q && !agents.length}<p class="omitted">Nothing matches.</p>{/if}
  {#if scene.omittedAgents}<p class="omitted">+{scene.omittedAgents} more on Agents</p>{/if}
  {/if}
</nav>

<style>
  .rail { min-height: 0; overflow-y: auto; padding: var(--sp-2); border-right: 1px solid var(--border); background: var(--surface); }
  .rail.collapsed { padding: var(--sp-2) 6px; }
  .head { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 2px; }
  .find { grid-column: 1 / -1; display: block; min-width: 0; margin: 0 var(--sp-1) var(--sp-1); }
  .find input { width: 100%; min-height: 28px; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); color: var(--text); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .find input:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .fold { display: inline-grid; place-items: center; width: 28px; height: 28px; min-height: 0; padding: 0; margin-left: auto; color: var(--muted); }
  .collapsed .fold { margin: 0 auto; }
  .fold:hover { color: var(--text); }
  .dots { margin-top: var(--sp-2); justify-items: center; gap: 4px; }
  .dotbtn { justify-content: center; width: 30px; min-height: 30px; padding: 0; }
  .dotbtn i { width: 12px; height: 12px; border-radius: 50%; background: var(--agent); }
  .dotbtn.dormant i { background: transparent; border: 1.5px dashed var(--muted); }
  .dotbtn .dot { position: absolute; top: 4px; right: 4px; }
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
