<script lang="ts">
  // Details for the selected port: header, tabs, scrolling body with edge shadows and a sticky
  // action footer. Used as the right-hand pane (wide windows) and inside a sheet (narrow).
  import { fade } from "svelte/transition";
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import Tabs, { panelId, tabId } from "../ui/Tabs.svelte";
  import ScrollArea from "../ui/ScrollArea.svelte";
  import Kbd from "../ui/Kbd.svelte";
  import DetailHeader from "./DetailHeader.svelte";
  import OverviewPanel from "./OverviewPanel.svelte";
  import ConnectionsPanel from "./ConnectionsPanel.svelte";
  import ProcessPanel from "./ProcessPanel.svelte";
  import NetworkPanel from "./NetworkPanel.svelte";
  import CommandsPanel from "./CommandsPanel.svelte";
  import type { Explanation, Graph, GraphNode, PortEntry } from "../../lib/types";
  import { dependencies, dependents, nodeForEntry } from "../../lib/graph";
  import { canOpen } from "../../lib/format";
  import { detailTabs, resolveTab, stopState, type DetailTab } from "../../lib/detail";
  import { tooltip } from "../../lib/tooltip";

  let {
    entry,
    explanation,
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
  }: {
    entry: PortEntry | null;
    explanation: Explanation | null;
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
  $effect(() => { void active; void entry?.id; scroller?.scrollTo({ top: 0 }); });
</script>

<aside class="pane" class:drawer aria-label="Port details">
  {#if !entry}
    <div class="placeholder">
      <div class="ph-art" aria-hidden="true"><span class="ring r1"></span><span class="ring r2"></span><span class="core"><Icon name="radar" size={22} /></span></div>
      <h3>Select a port</h3>
      <p>See who owns it, why it's busy, and exactly what Stop will do before anything happens.</p>
      <div class="ph-keys"><span><Kbd keys={["↑", "↓"]} size="sm" /> move</span><span><Kbd keys={[mod, "K"]} size="sm" /> commands</span><span><Kbd keys="?" size="sm" /> shortcuts</span></div>
    </div>
  {:else}
    <DetailHeader {entry} {pinned} {onpin} {oncopy} {onclose} />
    <Tabs {tabs} bind:value={tab} {base} label="Details sections" />
    {#key entry.id + active}
      <div class="body" in:fade={{ duration: reduced ? 0 : 120 }}>
        <ScrollArea bind:el={scroller}>
          <div role="tabpanel" id={panelId(base, active)} aria-labelledby={tabId(base, active)} class="panel">
            {#if active === "overview"}<OverviewPanel {entry} {explanation} {loading} {node} />
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
  .why { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-xs); color: var(--muted); font-weight: 550; }

  .placeholder { margin: auto; text-align: center; max-width: 300px; color: var(--muted); padding: var(--sp-6); }
  .placeholder h3 { color: var(--text); margin: var(--sp-5) 0 var(--sp-1); font-size: var(--fs-md); }
  .placeholder p { margin: 0 0 var(--sp-5); line-height: 1.5; }
  .ph-keys { display: inline-flex; flex-wrap: wrap; justify-content: center; gap: 6px 14px; font-size: var(--fs-xs); }
  .ph-keys span { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .ph-art { position: relative; width: 84px; height: 84px; margin: 0 auto; display: grid; place-items: center; }
  .ring { position: absolute; inset: 0; border-radius: 50%; border: 1px solid var(--accent); opacity: 0; animation: ping 2.8s var(--ease) infinite; }
  .ring.r2 { animation-delay: 1.4s; }
  .core { width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); }
  @keyframes ping { 0% { transform: scale(0.55); opacity: 0.6; } 100% { transform: scale(1.15); opacity: 0; } }
</style>
