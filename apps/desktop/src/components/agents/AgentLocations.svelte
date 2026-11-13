<script lang="ts">
  // Documented skill, context, rule and MCP locations. Metadata only: a present path
  // is not proof the agent loaded it, and a lower rank is a more specific layer, not a winner.
  import ExportActions from "../ui/ExportActions.svelte";
  import { filterPlaces, formatModified, PLACE_KIND, PLACE_STATUS, placeEntries, placeLayers, placesSummary, type PlaceFilter } from "../../lib/agent-memory";
  import { humanBytes, tildify } from "../../lib/format";
  import type { Agent, CapabilityPlace } from "../../lib/types";

  let { agent, oninspect }: { agent: Agent; oninspect?: () => void } = $props();

  const capabilities = $derived(agent.capabilities);
  const places = $derived(capabilities?.locations ?? []);
  let filter = $state<PlaceFilter>("all");
  let copied = $state<string | null>(null);
  let open = $state<Record<string, boolean>>({});
  const shown = $derived(filterPlaces(places, filter));
  const layers = $derived(placeLayers(shown));
  const filters = [
    ["all", "All"],
    ["present", "Present only"],
    ["skills", "Skills"],
    ["agent_memory", "Context / Memory"],
    ["rules", "Rules"],
    ["mcp_config", "MCP config"],
    ["instructions", "Commands & prompts"],
    ["builtin", "Built-in"],
    ["plugin", "Plugin"],
  ] as const;

  const filterCounts = $derived(
    Object.fromEntries(filters.map(([id]) => [id, filterPlaces(places, id).length])) as Record<string, number>,
  );

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copied = path;
      setTimeout(() => { if (copied === path) copied = null; }, 1600);
    } catch {
      copied = null;
    }
  }

  function pathlessDescription(place: CapabilityPlace): string {
    if (place.kind === "builtin") return "Built-in tools ship inside the product (no file on disk).";
    if (place.kind === "plugin") return "Installed plugins and cloud skills (not walked on disk).";
    return "No filesystem path recorded.";
  }
</script>

<section class="locations" aria-labelledby="agent-locations-{agent.id}">
  <h4 id="agent-locations-{agent.id}">Locations &amp; layers <span class="n">{places.length}</span></h4>
  {#if !capabilities?.inspected}
    <p class="empty">Skills, context, rules, MCP config and built-ins for {agent.name} stay hidden until configuration is read. Reading lists paths, counts and timestamps. It does not open chats or secret values.</p>
    {#if capabilities?.limits?.length}<p class="why">{capabilities.limits[0]}</p>{/if}
    {#if oninspect}<button type="button" class="link" onclick={oninspect}>Read configuration</button>{/if}
  {:else if !places.length}
    <p class="empty">No documented locations were reported for {agent.name}.</p>
    {#if capabilities.limits.length}<p class="why">{capabilities.limits[0]}</p>{/if}
  {:else}
    <p class="why">A present file is not proof it was loaded. A lower rank is the more specific documented layer. holdmap does not pick a winner.</p>
    <div class="filters" role="toolbar" aria-label="Location filters">
      {#each filters as [id, label] (id)}
        {@const count = filterCounts[id] ?? 0}
        {#if id === "all" || id === "present" || count > 0}
          <button type="button" aria-pressed={filter === id} onclick={() => (filter = id)}>
            <span>{label}</span>
            <span class="fcount">{count}</span>
          </button>
        {/if}
      {/each}
    </div>
    {#if !layers.length}
      <p class="empty">Nothing in this report matches that filter.</p>
    {/if}
    {#each layers as layer (layer.scope)}
      <div class="layer">
        <p class="layer-name">{layer.label} <span class="rank" title="Lower rank is the more specific documented layer. This is not a resolved winner.">Rank {layer.rank}</span></p>
        <ul class="place-list">
          {#each layer.places as place (place.id)}
            {@const listed = placeEntries(place)}
            <li class="place" class:place-missing={place.status === "missing"}>
              <div class="place-head">
                <span class="pills">
                  <span class="pill status-{place.status}">{PLACE_STATUS[place.status] ?? place.status}</span>
                  {#if place.symlink_target}<span class="pill status-symlink">Symlink</span>{/if}
                  <span class="kind">{PLACE_KIND[place.kind] ?? place.kind}</span>
                </span>
                {#if place.path}
                  <button type="button" class="copy" onclick={() => copyPath(place.path)}>{copied === place.path ? "Copied" : "Copy path"}</button>
                {/if}
              </div>
              {#if place.path}
                <span class="path mono">{tildify(place.path)}</span>
              {:else}
                <span class="path">{pathlessDescription(place)}</span>
              {/if}
              {#if place.symlink_target}
                <span class="meta mono">Target → {tildify(place.symlink_target)}</span>
              {/if}
              <div class="place-foot">
                {#if !place.path}
                  {#if place.kind === "builtin" && listed.length}
                    <span class="meta">{listed.length} documented {listed.length === 1 ? "tool" : "tools"}</span>
                  {/if}
                {:else if place.status === "missing"}
                  <span class="meta">Not found on disk</span>
                {:else}
                  <span class="meta">{place.file_count == null ? "count unknown" : `${place.file_count} ${place.file_count === 1 ? "entry" : "entries"}`} · {place.byte_size == null ? "size unknown" : humanBytes(place.byte_size)} · {formatModified(place.modified_unix_ms)}</span>
                {/if}
                {#if listed.length}
                  <button type="button" class="entries" aria-expanded={!!open[place.id]} onclick={() => (open[place.id] = !open[place.id])}>
                    {open[place.id] ? "Hide" : "Show"} {listed.length} {listed.length === 1 ? "entry" : "entries"}
                  </button>
                {/if}
              </div>
              {#if place.status_reason}<span class="why">{place.status_reason}</span>{/if}
              {#if listed.length && open[place.id]}
                <ul class="names">
                  {#each listed as entry (entry.name)}
                    <li class="entry-row">
                      <span class="entry-name mono">{entry.name}</span>
                      {#if entry.kindLabel || entry.bytes != null || entry.modified}
                        <span class="entry-meta">
                          {#if entry.kindLabel}<span class="entry-kind">{entry.kindLabel}</span>{/if}
                          {#if entry.bytes != null}<span>{humanBytes(entry.bytes)}</span>{/if}
                          {#if entry.modified}<span>{entry.modified}</span>{/if}
                        </span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </li>
          {/each}
        </ul>
      </div>
    {/each}
    <ExportActions label="Export locations" copy={() => placesSummary(places)} files={[{ label: "JSON", filename: `${agent.product}-locations.json`, build: () => places }]} />
  {/if}
</section>

<style>
  .locations { display: grid; gap: 8px; min-width: 0; }
  h4 { margin: 8px 0 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--muted); }
  .n { font-variant-numeric: tabular-nums; color: var(--faint); }
  .empty, .why, .meta { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }
  .filters { display: flex; flex-wrap: wrap; gap: 4px; }
  .filters button, .copy, .entries, .link { display: inline-flex; align-items: center; gap: 5px; border: 1px solid var(--border); background: var(--surface); color: var(--text-2); border-radius: var(--r-full); padding: 2px 8px; font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; flex: none; }
  .filters button[aria-pressed="true"] { background: var(--surface-2); color: var(--text); border-color: transparent; font-weight: var(--fw-semibold); }
  .fcount { color: var(--faint); font-variant-numeric: tabular-nums; font-weight: var(--fw-regular); }
  .filters button[aria-pressed="true"] .fcount { color: var(--muted); }
  .link { border: 0; padding: 0; background: none; color: var(--accent); text-decoration: underline; text-underline-offset: 2px; justify-self: start; }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .layer { display: grid; gap: 6px; min-width: 0; }
  .layer-name { margin: 2px 0 0; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; color: var(--text); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .place-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .place { min-width: 0; display: grid; gap: 5px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); }
  .place-missing { opacity: 0.78; border-style: dashed; }
  .place-head { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; min-width: 0; }
  .copy { margin-left: auto; }
  .pills { display: inline-flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .pill, .kind, .rank { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
  .status-present { background: var(--ok-soft); color: var(--ok); }
  .status-missing { color: var(--muted); }
  .status-unreadable, .status-skipped { background: var(--warn-soft); color: var(--warn); }
  .status-symlink { background: var(--surface-2); color: var(--text); }
  .path { min-width: 0; overflow-wrap: anywhere; color: var(--text); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .place-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 8px; min-width: 0; }
  .names { list-style: none; margin: 2px 0 0; padding: 6px 0 0; border-top: 1px solid var(--border); display: grid; gap: 4px; }
  .entry-row { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 4px 8px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .entry-name { color: var(--text); overflow-wrap: anywhere; }
  .entry-meta { display: inline-flex; flex-wrap: wrap; gap: 6px; align-items: center; color: var(--muted); font-variant-numeric: tabular-nums; }
  .entry-kind { padding: 0 5px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
</style>
