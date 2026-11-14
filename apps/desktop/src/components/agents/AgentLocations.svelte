<script lang="ts">
  // Documented skills, context/memory, rules, MCP config, commands/prompts and built-ins
  // grouped by documented precedence layer. Present on disk is not proof an agent loaded it.
  import Icon from "../Icon.svelte";
  import ConsentGate from "../ui/ConsentGate.svelte";
  import ExportActions from "../ui/ExportActions.svelte";
  import { PLACE_KIND, PLACE_STATUS, combineText, filterPlaces, placeEntries, placeFacts, placeLayers, placesSummary, type PlaceFilter } from "../../lib/agent-memory";
  import { humanBytes, tildify } from "../../lib/format";
  import type { Agent } from "../../lib/types";

  const KIND_ICON: Record<string, string> = {
    instructions: "panel",
    rules: "shield",
    settings: "sliders",
    mcp_config: "plug",
    skills: "sparkles",
    commands: "terminal",
    subagents: "bot",
    hooks: "zap",
    extensions: "box",
    agent_memory: "history",
    session_data: "clock",
    secret_store: "lock",
    builtin: "cpu",
    plugin: "box",
  };

  let { agent, oninspect }: { agent: Agent; oninspect?: () => void } = $props();
  const capabilities = $derived(agent.capabilities);
  const places = $derived(capabilities?.locations ?? []);
  let filter = $state<PlaceFilter>("all");
  let copied = $state<string | null>(null);
  let open = $state<Record<string, boolean>>({});
  const shown = $derived(filterPlaces(places, filter));
  const layers = $derived(placeLayers(shown));
  const presentCount = $derived(places.filter((p) => p.status === "present" || p.present).length);
  const missingCount = $derived(places.filter((p) => p.status === "missing").length);

  const kindFilters = Object.entries(PLACE_KIND) as [PlaceFilter, string][];



  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copied = path;
      setTimeout(() => { if (copied === path) copied = null; }, 1600);
    } catch {
      copied = null;
    }
  }


</script>

<section class="locations" aria-labelledby="agent-locations-{agent.id}">
  <div class="loc-head">
    <h4 id="agent-locations-{agent.id}"><Icon name="layers" size={13} />Locations &amp; layers <span class="n">{places.length}</span></h4>
    {#if capabilities?.inspected && places.length}
      <span class="counts-summary">
        <span class="c-ok">{presentCount} present</span>{#if missingCount}{" "}· <span>{missingCount} not found</span>{/if}
      </span>
    {/if}
  </div>

  {#if !capabilities?.inspected}
    <div class="gate">
      <p class="empty">Documented locations for {agent.name} (instructions, rules, MCP config, skills, agent memory and session data) are checked when you read local config.</p>
      {#if capabilities?.limits?.length}<p class="why">{capabilities.limits[0]}</p>{/if}
      {#if oninspect}<ConsentGate onconsent={oninspect} />{/if}
    </div>
  {:else if !places.length}
    <p class="empty">No documented locations were reported for {agent.name}.</p>
    {#if capabilities.limits.length}<p class="why">{capabilities.limits[0]}</p>{/if}
  {:else}
    <p class="why">A present file is not proof it was loaded. Layers are listed from most specific to broadest; each row says how its layers combine.</p>
    <div class="filter-bar" role="toolbar" aria-label="Location filters">
      <div class="seg-group">
        <button type="button" aria-pressed={filter === "all"} onclick={() => (filter = "all")}>All <span class="fcount">{places.length}</span></button>
        <button type="button" aria-pressed={filter === "present"} onclick={() => (filter = "present")}>Present only <span class="fcount">{presentCount}</span></button>
      </div>
      <div class="kind-chips">
        {#each kindFilters as [id, label] (id)}
          {@const count = filterPlaces(places, id).length}
          {#if count > 0}
            <button type="button" aria-pressed={filter === id} onclick={() => (filter = filter === id ? "all" : id)}>
              <span>{label}</span>
              <span class="fcount">{count}</span>
            </button>
          {/if}
        {/each}
      </div>
    </div>

    {#if !layers.length}
      <p class="empty">Nothing in this report matches that filter.</p>
    {/if}

    <div class="ladder">
      {#each layers as layer (layer.scope)}
        <div class="layer">
          <p class="layer-name">
            <i class="node-dot" aria-hidden="true"></i>
            <span>{layer.label}</span>
          </p>
          <ul class="place-list">
            {#each layer.places as place (place.id)}
              {@const listed = placeEntries(place)}
              {@const isMissing = place.status === "missing"}
              <li class="place" class:place-missing={isMissing} class:place-present={place.status === "present"}>
                <div class="place-head">
                  <span class="pills">
                    <span class="kind-ic" aria-hidden="true"><Icon name={KIND_ICON[place.kind] ?? "folder"} size={12} /></span>
                    <span class="kind">{place.label || (PLACE_KIND[place.kind] ?? place.kind)}</span>
                    {#if place.label}<span class="pill">{PLACE_KIND[place.kind] ?? place.kind}</span>{/if}
                    <span class="pill status-{place.status}">{PLACE_STATUS[place.status] ?? place.status}</span>
                    {#if place.symlink_target}<span class="pill status-symlink">Symlink</span>{/if}
                  </span>
                  {#if place.path}
                    <button type="button" class="copy" onclick={() => copyPath(place.path)} aria-label={copied === place.path ? "Copied" : "Copy path"}>
                      <Icon name={copied === place.path ? "check" : "copy"} size={11} />
                      <span>{copied === place.path ? "Copied" : "Copy path"}</span>
                    </button>
                  {/if}
                </div>

                {#if place.path}
                  <span class="path mono" title={place.path}>{tildify(place.path)}</span>
                {:else}
                  <span class="path muted-desc">{place.kind === "builtin" ? "Built-in tools ship inside the product (no file on disk)." : place.kind === "plugin" ? "Installed plugins and cloud skills (not walked on disk)." : place.scope === "account" ? "Kept in the account or app settings, not on disk." : "Not documented for this operating system."}</span>
                {/if}

                {#if place.symlink_target}
                  <span class="meta mono">Target → {tildify(place.symlink_target)}</span>
                {/if}

                {#if !isMissing || listed.length || place.status_reason}
                  <div class="place-foot">
                    {#if !place.path}
                      {#if place.kind === "builtin" && listed.length}
                        <span class="meta">{listed.length} documented {listed.length === 1 ? "tool" : "tools"}</span>
                      {/if}
                    {:else if isMissing}
                      <span class="meta">Not found on disk</span>
                    {:else}
                      <span class="meta">{placeFacts(place)}</span>
                    {/if}
                    {#if listed.length}
                      <button type="button" class="entries" aria-expanded={!!open[place.id]} onclick={() => (open[place.id] = !open[place.id])}>
                        {open[place.id] ? "Hide" : "Show"} {listed.length} {listed.length === 1 ? "entry" : "entries"}
                      </button>
                    {/if}
                  </div>
                {:else}
                  <span class="meta">Not found on disk</span>
                {/if}

                {#if place.status_reason}<span class="why">{place.status_reason}</span>{/if}
                {#if place.note}<span class="why">{place.note}</span>{/if}
                {#if combineText(place) && place.path}<span class="why combine">How layers combine: {combineText(place)}{place.custom_home ? ` · custom home from ${place.custom_home}` : ""}</span>{/if}

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
    </div>

    <ExportActions label="Export locations" copy={() => placesSummary(places)} files={[{ label: "JSON", filename: `${agent.product}-locations.json`, build: () => places }]} />
  {/if}
</section>

<style>
  .locations { display: grid; gap: 8px; min-width: 0; }
  .loc-head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 6px; margin-top: 4px; }
  h4 { margin: 0; display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .n { font-variant-numeric: tabular-nums; color: var(--muted); font-weight: var(--fw-medium); }
  .counts-summary { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .c-ok { color: var(--ok); font-weight: var(--fw-medium); }

  .gate { display: grid; gap: 8px; padding: 10px 12px; border: 1px dashed var(--border-strong); border-radius: var(--r-md); background: var(--surface-2); }

  .empty, .why, .meta { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); overflow-wrap: anywhere; }

  .filter-bar { display: grid; gap: 6px; }
  .seg-group { display: inline-flex; justify-self: start; padding: 2px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); gap: 2px; }
  .seg-group button { display: inline-flex; align-items: center; gap: 5px; border: 0; background: transparent; color: var(--text-2); border-radius: var(--r-full); padding: 2px 9px; font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .seg-group button[aria-pressed="true"] { background: var(--surface); color: var(--text); font-weight: var(--fw-semibold); box-shadow: var(--shadow-sm); }

  .kind-chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .kind-chips button, .copy, .entries { display: inline-flex; align-items: center; gap: 4px; border: 1px solid var(--border); background: var(--surface); color: var(--text-2); border-radius: var(--r-full); padding: 2px 8px; font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; flex: none; }
  .kind-chips button:hover, .copy:hover, .entries:hover { border-color: var(--border-strong); color: var(--text); }
  .kind-chips button[aria-pressed="true"] { background: var(--accent-soft); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); font-weight: var(--fw-semibold); }
  .fcount { color: var(--faint); font-variant-numeric: tabular-nums; font-weight: var(--fw-regular); }
  .kind-chips button[aria-pressed="true"] .fcount { color: var(--accent); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }

  .ladder { display: grid; gap: 10px; padding-left: 4px; }
  .layer { position: relative; display: grid; gap: 6px; min-width: 0; padding-left: 12px; border-left: 1px solid var(--border-strong); }
  .layer-name { margin: 0; display: flex; align-items: center; gap: 6px; color: var(--text); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); }
  .node-dot { position: absolute; left: -4px; width: 7px; height: 7px; border-radius: 50%; background: var(--agent, var(--accent)); border: 1.5px solid var(--surface); }

  .place-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 5px; }
  .place { min-width: 0; display: grid; gap: 4px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); }
  .place-present { border-left: 2px solid var(--ok); }
  .place-missing { padding: 6px 9px; border-style: dashed; background: var(--surface-2); opacity: 0.82; }

  .place-head { display: flex; align-items: center; justify-content: space-between; gap: 6px; min-width: 0; }
  .pills { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; min-width: 0; }
  .kind-ic { display: inline-flex; color: var(--muted); flex: none; }
  .kind { color: var(--text); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); }
  .pill { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); flex: none; }
  .status-present { background: var(--ok-soft); color: var(--ok); font-weight: var(--fw-medium); }
  .status-missing { background: var(--surface); color: var(--muted); }
  .status-unreadable, .status-skipped { background: var(--warn-soft); color: var(--warn); }
  .status-symlink { background: var(--accent-soft); color: var(--accent); }
  .copy { margin-left: auto; flex: none; }

  .path { min-width: 0; overflow-wrap: anywhere; color: var(--text); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .path.muted-desc { font-family: var(--font); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .place-missing .path { color: var(--text-2); }
  .place-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 4px 8px; min-width: 0; }

  .names { list-style: none; margin: 3px 0 0; padding: 6px 0 0 8px; border-top: 1px solid var(--border); border-left: 2px solid var(--surface-3); display: grid; gap: 4px; }
  .entry-row { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 2px 8px; min-width: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .entry-name { color: var(--text); overflow-wrap: anywhere; }
  .entry-meta { display: inline-flex; flex-wrap: wrap; gap: 6px; align-items: center; color: var(--muted); font-variant-numeric: tabular-nums; }
  .entry-kind { padding: 0 5px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
</style>
