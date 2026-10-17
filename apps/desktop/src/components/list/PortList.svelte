<script lang="ts">
  // The port list body: loading skeletons, empty and error states, grouped rows and the
  // visibility foot-note. Selection, stops and checks are owned by the app and passed in.
  import Icon from "../Icon.svelte";
  import EmptyState from "../EmptyState.svelte";
  import GroupHeader from "./GroupHeader.svelte";
  import PortRow from "./PortRow.svelte";
  import { Button } from "../ui";
  import type { Density } from "../../lib/rows";
  import type { PortSection, PortStats, Sort } from "../../lib/port-list";
  import type { Explanation, PortEntry, Snapshot } from "../../lib/types";

  let {
    listEl = $bindable(),
    snapshot,
    error,
    refreshing,
    sections,
    ordered,
    collapsed,
    sort,
    density,
    pins,
    links,
    usage,
    selectedId,
    busy,
    query,
    queryPort,
    filtersActive,
    stats,
    checkingPort,
    freePort,
    portCheckError,
    onretry,
    oncheckagain,
    onclearfilters,
    onfindfree,
    oncopy,
    onpinfree,
    ontogglesection,
    onselect,
    onstop,
    onopen,
    onpin,
  }: {
    listEl?: HTMLDivElement;
    snapshot: Snapshot | null;
    error: string | null;
    refreshing: boolean;
    sections: PortSection[];
    ordered: PortEntry[];
    collapsed: ReadonlySet<string>;
    sort: Sort;
    density: Density;
    pins: ReadonlySet<number>;
    links: ReadonlyMap<string, number>;
    usage: Record<string, number[]>;
    selectedId: string | null;
    busy: Record<string, boolean>;
    query: string;
    queryPort: number | null;
    filtersActive: boolean;
    stats: PortStats;
    checkingPort: number | null;
    freePort: Explanation | null;
    portCheckError: string | null;
    onretry: () => void;
    oncheckagain: () => void;
    onclearfilters: () => void;
    onfindfree: (near: number) => void;
    oncopy: (text: string, what: string) => void;
    onpinfree: () => void;
    ontogglesection: (id: string, collapsed: boolean) => void;
    onselect: (e: PortEntry) => void;
    onstop: (e: PortEntry) => void;
    onopen: (e: PortEntry) => void;
    onpin: (e: PortEntry) => void;
  } = $props();
</script>

<div class="list {density}" id="port-list" role="listbox" aria-label="Ports in use" aria-activedescendant={selectedId ? "row-" + selectedId : undefined} tabindex="0" bind:this={listEl}>
  {#if error && !snapshot}
    <EmptyState tone="err" title="Couldn't read the socket table">
      <p class="selectable">{error}</p>
      <Button variant="primary" icon="refresh" loading={refreshing} loadingText="Retrying…" onclick={onretry}>Try again</Button>
    </EmptyState>
  {:else if !snapshot}
    <div class="skeletons" aria-busy="true" aria-label="Loading ports">
      <div class="sk-gh shimmer"></div>
      {#each Array(9) as _, i}
        <div class="sk-row" style="animation-delay: {i * 70}ms; opacity: {1 - i * 0.08}">
          <div class="shimmer" style="height:7px;width:7px;border-radius:50%"></div>
          <div class="shimmer" style="height:12px;width:38px"></div>
          <div style="display:flex;gap:8px;align-items:center"><div class="shimmer" style="height:22px;width:22px;border-radius:6px"></div><div class="shimmer" style="height:11px;width:{40 + ((i * 37) % 35)}%"></div></div>
          <div class="shimmer" style="height:9px;width:{45 + ((i * 23) % 30)}%"></div>
        </div>
      {/each}
    </div>
  {:else if ordered.length === 0}
    {#if error}
      <EmptyState tone="warn" title="Port status unavailable">
        <p>The last scan couldn't be updated. Check again before starting your server.</p>
      </EmptyState>
    {:else if checkingPort !== null && !freePort}
      <EmptyState icon="radar" title="Checking port {checkingPort}…">
        <p aria-busy="true">Checking its current availability.</p>
      </EmptyState>
    {:else if portCheckError && queryPort !== null}
      <EmptyState tone="err" title="Couldn't check port {queryPort}">
        <p class="selectable">{portCheckError}</p>
        <Button icon="refresh" onclick={oncheckagain}>Check again</Button>
      </EmptyState>
    {:else if freePort && freePort.status === "free"}
      <EmptyState tone="ok" icon="check" title="Port {freePort.port} is free">
        <p aria-busy={checkingPort !== null || undefined}>{checkingPort !== null ? "Checking availability…" : "Nothing is listening on it — start your server there:"}</p>
        <Button icon="copy" onclick={() => oncopy(`holdmap run -p ${freePort?.port} -- npm run dev`, "Command")}><code>holdmap run -p {freePort.port} -- npm run dev</code></Button>
        <Button variant="ghost" icon="star" onclick={onpinfree}>Pin :{freePort.port} and watch it</Button>
      </EmptyState>
    {:else if freePort}
      <EmptyState tone="warn" title="Port {freePort.port}">
        <p class="selectable">{freePort.headline}</p>
        <p class="selectable">{freePort.recommendation}</p>
      </EmptyState>
    {:else if filtersActive}
      <EmptyState title="No ports match">
        <p>Nothing matches {query ? `“${query}”` : "these filters"}{stats.total ? ` — ${stats.total} port${stats.total === 1 ? " is" : "s are"} hidden by your search and filters` : ""}.</p>
        <Button kbd="Esc" onclick={onclearfilters}>Clear search & filters</Button>
      </EmptyState>
    {:else}
      <EmptyState tone="ok" title="All quiet">
        <p>Nothing is listening right now. Start a dev server and it shows up here within a few seconds.</p>
        <Button icon="sparkles" onclick={() => onfindfree(3000)}>Find a free port</Button>
      </EmptyState>
    {/if}
  {:else}
    {#each sections as s (s.id)}
      {@const isCollapsed = !!s.title && collapsed.has(s.id)}
      <div class="lgroup" role="group" aria-labelledby={s.title ? `gl-${s.id}` : undefined} aria-label={s.title ? undefined : "Ports"}>
        {#if s.title}
          <GroupHeader id={s.id} title={s.title} count={s.items.length} hint={s.hint} showHint={sort === "cluster" && s.id !== "pinned"} collapsed={isCollapsed} ontoggle={() => ontogglesection(s.id, !isCollapsed)} />
        {/if}
        {#if !isCollapsed}
          <div class="lrows" id="grp-{s.id}" role="presentation">
            {#each s.items as e (e.id)}
              <PortRow entry={e} {density} pinned={pins.has(e.port)} links={links.get(e.id) ?? 0} usage={usage[e.id] ?? []} selected={e.id === selectedId} busy={!!busy[e.id]}
                posinset={ordered.indexOf(e) + 1} setsize={ordered.length}
                onselect={() => { onselect(e); listEl?.focus({ preventScroll: true }); }} onstop={() => onstop(e)} onopen={() => onopen(e)} onpin={() => onpin(e)} />
            {/each}
          </div>
        {/if}
      </div>
    {/each}
    {#if snapshot.hidden_sockets > 0 || !snapshot.docker_available}
      <div class="foot-note">
        {#if snapshot.hidden_sockets > 0}<span><Icon name="lock" size={12} />{snapshot.hidden_sockets} socket{snapshot.hidden_sockets === 1 ? "" : "s"} owned by other users — run with admin rights to see them</span>{/if}
        {#if !snapshot.docker_available}<span><Icon name="box" size={12} />Docker isn't running — container names unavailable</span>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .list { overflow-y: auto; padding: 0 0 var(--sp-6); outline: none; container: portlist / inline-size; min-height: 0; }
  .lgroup + .lgroup { margin-top: var(--sp-2); }
  .list:focus-visible:not([aria-activedescendant]) { box-shadow: inset 0 0 0 2px var(--ring); }
  .foot-note { display: flex; flex-direction: column; gap: 6px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); padding: var(--sp-5) var(--sp-5) 0; }
  .foot-note span { display: inline-flex; align-items: center; gap: 6px; }
  .skeletons { padding: var(--sp-3) var(--sp-2); }
  .sk-gh { height: 9px; width: 110px; margin: var(--sp-4) var(--sp-5) var(--sp-3); }
  .sk-row { display: grid; grid-template-columns: 8px 48px minmax(0, 1.4fr) minmax(0, 1fr); gap: var(--sp-3); align-items: center; height: 44px; padding: 0 var(--sp-3); }
</style>
