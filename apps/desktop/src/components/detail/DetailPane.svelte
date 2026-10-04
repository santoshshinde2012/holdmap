<script lang="ts">
  // Details for the selected port: header, tabs, scrolling body with edge shadows and a sticky
  // action footer. Used as the right-hand pane (wide windows) and inside a sheet (narrow).
  import { fade } from "svelte/transition";
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import Tabs, { panelId, tabId } from "../ui/Tabs.svelte";
  import ScrollArea from "../ui/ScrollArea.svelte";
  import DetailHeader from "./DetailHeader.svelte";
  import AtAGlance from "./AtAGlance.svelte";
  import OverviewPanel from "./OverviewPanel.svelte";
  import ConnectionsPanel from "./ConnectionsPanel.svelte";
  import ProcessPanel from "./ProcessPanel.svelte";
  import NetworkPanel from "./NetworkPanel.svelte";
  import CommandsPanel from "./CommandsPanel.svelte";
  import type { Explanation, Graph, GraphNode, HttpInfo, PortEntry } from "../../lib/types";
  import { dependencies, dependents, nodeForEntry } from "../../lib/graph";
  import { canOpen } from "../../lib/format";
  import { detailTabs, resolveTab, stopState, type DetailTab } from "../../lib/detail";
  import { tooltip } from "../../lib/tooltip";

  let {
    entry,
    explanation,
    http = null,
    loading,
    busy,
    drawer = false,
    tab = $bindable("overview"),
    onstop,
    onkill,
    onopen,
    oncopy,
    onclose,
    graph = null,
    pinned = false,
    onpin,
    onstopcluster,
    onselectnode,
    mod = "Ctrl",
    entries = [],
    onselectentry = () => {},
  }: {
    entry: PortEntry | null;
    explanation: Explanation | null;
    http?: HttpInfo | null;
    loading: boolean;
    busy: boolean;
    drawer?: boolean;
    tab?: DetailTab;
    onstop: () => void;
    onkill: () => void;
    onopen: () => void;
    oncopy: (text: string, what: string) => void;
    onclose?: () => void;
    graph?: Graph | null;
    pinned?: boolean;
    onpin?: () => void;
    onstopcluster?: (name: string) => void;
    onselectnode?: (n: GraphNode) => void;
    mod?: string;
    /** Every port, for the summary shown while nothing is selected. */
    entries?: PortEntry[];
    onselectentry?: (e: PortEntry) => void;
  } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const node = $derived(entry ? nodeForEntry(graph, entry.id) : null);
  const cluster = $derived(node?.cluster ? graph?.clusters.find((c) => c.id === node.cluster) ?? null : null);
  const deps = $derived(node && graph ? dependencies(graph, node.id) : []);
  const users = $derived(node && graph ? dependents(graph, node.id) : []);
  const tabs = $derived(entry ? detailTabs(entry, explanation, { deps: deps.length, users: users.length, cluster: !!cluster }) : []);
  const active = $derived(resolveTab(tab, tabs));
  const stop = $derived(entry ? stopState(entry, explanation?.plan ?? null) : null);
  const base = $derived(drawer ? "dd" : "dp");
  let scroller: HTMLDivElement | undefined = $state();
  // Back to the top on a new port or tab only; a poll's fresh entry object must not scroll.
  const entryId = $derived(entry?.id ?? null);
  $effect(() => { void active; void entryId; scroller?.scrollTo({ top: 0 }); });
</script>

<aside class="pane" class:drawer aria-label="Port details">
  {#if !entry}
    <AtAGlance {entries} onselect={onselectentry} {mod} />
  {:else}
    <DetailHeader {entry} {pinned} {onpin} {oncopy} {onclose} />
    <Tabs {tabs} bind:value={tab} {base} label="Details sections" />
    {#key entryId + active}
      <div class="body" in:fade={{ duration: reduced ? 0 : 120 }}>
        <ScrollArea bind:el={scroller}>
          <div role="tabpanel" id={panelId(base, active)} aria-labelledby={tabId(base, active)} class="panel">
            {#if active === "overview"}<OverviewPanel {entry} {explanation} {http} {loading} {node} />
            {:else if active === "connections" && node && graph}<ConnectionsPanel {graph} {node} {cluster} {deps} {users} {onstopcluster} {onselectnode} />
            {:else if active === "process"}<ProcessPanel {entry} {node} {oncopy} />
            {:else if active === "network"}<NetworkPanel {entry} {oncopy} />
            {:else if active === "commands"}<CommandsPanel {entry} {explanation} {oncopy} />{/if}
          </div>
        </ScrollArea>
      </div>
    {/key}
    <footer class="actions">
      {#if canOpen(entry)}<Button icon="external" kbd="O" onclick={onopen}>Open</Button>{/if}
      <span class="sp"></span>
      {#if stop && (entry.process || entry.container)}
        {#if stop.stoppable}
          {#if entry.process && !entry.container}<Button variant="ghost" icon="zap" onclick={onkill} disabled={busy} tip={{ text: "Skip SIGTERM and kill immediately", kbd: ["⇧", "⌫"] }}>Force kill</Button>{/if}
          <Button variant="danger" icon="stop" kbd="⌫" loading={busy} loadingText="Stopping…" onclick={onstop}>{entry.container ? "Stop container" : "Stop"}</Button>
        {:else if stop.overridable}
          <span class="why" use:tooltip={stop.reason}><Icon name="shield" size={13} />Protected</span>
          <Button variant="danger-outline" icon="stop" onclick={onstop} disabled={busy}>Stop…</Button>
        {:else}
          <span class="why"><Icon name={stop.reason?.startsWith("Needs") ? "lock" : "shield"} size={13} />{stop.reason}</span>
        {/if}
      {:else if stop?.reason}
        <span class="why"><Icon name="lock" size={13} />{stop.reason}</span>
      {/if}
    </footer>
  {/if}
</aside>

<style>
  .pane { display: flex; flex-direction: column; min-height: 0; height: 100%; background: var(--surface); border-left: 1px solid var(--border); min-width: 0; }
  .pane.drawer { border-left: 0; }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .panel { outline: none; }
  .actions { display: flex; align-items: center; gap: var(--sp-2); padding: var(--sp-3) var(--sp-4) var(--sp-3) var(--sp-5); border-top: 1px solid var(--border); background: var(--surface); flex-wrap: wrap; }
  .sp { flex: 1; }
  .why { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); font-weight: var(--fw-medium); }

</style>
