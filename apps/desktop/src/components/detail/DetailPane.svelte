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
  import ActionMenu, { type MenuItem } from "../ui/ActionMenu.svelte";
  import type { Explanation, Graph, GraphNode, HttpInfo, PortDetails, PortEntry } from "../../lib/types";
  import { dependencies, dependents, nodeForEntry } from "../../lib/graph";
  import { canOpen, url } from "../../lib/format";
  import { canRestart, curlCommand, detailTabs, killCommand, killHelpersCommand, projectFolder, resolveTab, stopState, type DetailTab } from "../../lib/detail";

  let {
    entry,
    explanation,
    http = null,
    loading,
    busy,
    drawer = false,
    tab = $bindable("overview"),
    onstop,
    onschedule,
    onkill,
    onopen,
    oncopy,
    onclose,
    onhide,
    graph = null,
    pinned = false,
    onpin,
    onstopcluster,
    onselectnode,
    mod = "Ctrl",
    entries = [],
    onselectentry = () => {},
    details = null,
    cpuTrend = [],
    memTrend = [],
    platform = "",
    onrestart,
    onreveal,
    oneditor,
  }: {
    entry: PortEntry | null;
    explanation: Explanation | null;
    http?: HttpInfo | null;
    loading: boolean;
    busy: boolean;
    drawer?: boolean;
    tab?: DetailTab;
    onstop: () => void;
    /** Review a local, non-forced port stop for a later time. */
    onschedule?: () => void;
    onkill: () => void;
    onopen: () => void;
    oncopy: (text: string, what: string) => void;
    onclose?: () => void;
    /** Docked pane: hide it so the graph or list can use the full width. */
    onhide?: () => void;
    graph?: Graph | null;
    pinned?: boolean;
    onpin?: () => void;
    onstopcluster?: (name: string) => void;
    onselectnode?: (n: GraphNode) => void;
    mod?: string;
    /** Every port, for the summary shown while nothing is selected. */
    entries?: PortEntry[];
    onselectentry?: (e: PortEntry) => void;
    /** Lazily loaded extras for this listener (connections, tree, bind risk); null until loaded. */
    details?: PortDetails | null;
    /** Recent CPU % and memory samples, oldest first, for the trend lines. */
    cpuTrend?: number[];
    memTrend?: number[];
    platform?: string;
    onrestart?: () => void;
    onreveal?: () => void;
    oneditor?: () => void;
  } = $props();

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const node = $derived(entry ? nodeForEntry(graph, entry.id) : null);
  const cluster = $derived(node?.cluster ? graph?.clusters.find((c) => c.id === node.cluster) ?? null : null);
  const deps = $derived(node && graph ? dependencies(graph, node.id) : []);
  const users = $derived(node && graph ? dependents(graph, node.id) : []);
  const tabs = $derived(entry ? detailTabs(entry, explanation, { deps: deps.length, users: users.length, cluster: !!cluster }) : []);
  const active = $derived(resolveTab(tab, tabs));
  const stop = $derived(entry ? stopState(entry, explanation?.plan ?? null) : null);
  const stopReady = $derived(!!explanation?.plan && !explanation.plan.blocked && explanation.plan.steps.length > 0 && !!stop?.stoppable);
  const schedulable = $derived(!!onschedule && stopReady && !!entry && !entry.protected && !entry.remote && !explanation?.plan?.allow_protected && !explanation?.plan?.steps.some((s) => s.action === "signal_processes" && s.force));
  const stopReason = $derived(stop?.reason ?? (!explanation?.plan ? (loading ? "Checking stop permissions…" : "Stop plan unavailable. Refresh to try again.") : !stopReady ? "Nothing to stop" : null));
  const base = $derived(drawer ? "dd" : "dp");
  const restartable = $derived(!!entry && !!onrestart && canRestart(entry, explanation?.plan ?? null));
  const fileManager = $derived(platform === "macos" ? "Finder" : platform === "windows" ? "Explorer" : "Files");
  const menu = $derived.by((): MenuItem[] => {
    if (!entry) return [];
    const e = entry;
    const out: MenuItem[] = [];
    const web = e.protocol === "tcp" && e.state === "listen";
    if (canOpen(e)) out.push({ id: "url", label: "Copy URL", icon: "link", hint: "C", run: () => oncopy(url(e), "URL") });
    if (web) out.push({ id: "curl", label: "Copy curl command", icon: "terminal", run: () => oncopy(curlCommand(e), "curl command") });
    const dir = projectFolder(e);
    if (dir && oneditor) out.push({ id: "editor", label: "Open folder in editor", icon: "code", run: oneditor });
    if (dir && onreveal) out.push({ id: "reveal", label: `Open folder in ${fileManager}`, icon: "folder", run: onreveal });
    if (dir) out.push({ id: "path", label: "Copy folder path", icon: "copy", run: () => oncopy(dir, "Path") });
    const kill = killCommand(e, platform);
    if (kill && stopReady && !e.protected) out.push({ id: "kill", label: e.container ? "Copy docker stop command" : "Copy kill command", icon: "copy", run: () => oncopy(kill, "Command") });
    const helpers = killHelpersCommand(e, platform);
    if (helpers) out.push({ id: "helpers", label: `Copy quit-helpers command (${e.helper_count} helper${e.helper_count === 1 ? "" : "s"})`, icon: "zap", run: () => oncopy(helpers, "Quit-helpers command") });
    if (stopReady && !busy && !loading && e.process && !e.container) out.push({ id: "force", label: "Force kill…", icon: "zap", hint: "⇧⌫", run: onkill });
    return out;
  });
  let scroller: HTMLDivElement | undefined = $state();
  // Back to the top on a new port or tab only; a poll's fresh entry object must not scroll.
  const entryId = $derived(entry?.id ?? null);
  $effect(() => { void active; void entryId; scroller?.scrollTo({ top: 0 }); });
</script>

<aside class="pane" class:drawer aria-label="Port details">
  {#if !entry}
    <AtAGlance {entries} onselect={onselectentry} {mod} {onhide} />
  {:else}
    <DetailHeader {entry} {pinned} {onpin} {oncopy} {onclose} {onhide} />
    <Tabs {tabs} value={active} onchange={(value) => tab = value} {base} label="Details sections" />
    {#key entryId + active}
      <div class="body" in:fade={{ duration: reduced ? 0 : 120 }}>
        <ScrollArea bind:el={scroller} padded={false}>
          <div role="tabpanel" id={panelId(base, active)} aria-labelledby={tabId(base, active)} class="panel">
            {#if active === "overview"}<OverviewPanel {entry} {explanation} {http} {loading} {node} {details} {cpuTrend} {memTrend} />
            {:else if active === "connections" && node && graph}<ConnectionsPanel {graph} {node} {cluster} {deps} {users} {onstopcluster} {onselectnode} />
            {:else if active === "process"}<ProcessPanel {entry} {node} {oncopy} {details} />
            {:else if active === "network"}<NetworkPanel {entry} {oncopy} {details} />
            {:else if active === "commands"}<CommandsPanel {entry} {explanation} {oncopy} />{/if}
          </div>
        </ScrollArea>
      </div>
    {/key}
    <footer class="actions" aria-label="Port actions">
      <div class="bar">
        {#if canOpen(entry) || restartable || menu.length}
          <div class="utilities">
            {#if canOpen(entry)}<Button size="sm" variant="ghost" icon="external" kbd="O" onclick={onopen}>Open</Button>{/if}
            {#if restartable}<Button size="sm" variant="ghost" icon="refresh" onclick={onrestart} disabled={busy || loading} tip={{ text: "Stop it, then run the same command again in the same folder" }}>Restart</Button>{/if}
            {#if menu.length}<ActionMenu items={menu} size="sm" />{/if}
          </div>
        {/if}
        <div class="stop" role="group" aria-label="Stop this port">
          {#if schedulable}<Button size="sm" variant="secondary" icon="clock" disabled={busy || loading} onclick={onschedule}>Schedule stop…</Button>{/if}
          <Button size="sm" variant={stop?.overridable ? "danger-outline" : "danger"} icon="stop" kbd={entry.container ? undefined : "⌫"} loading={busy} loadingText="Stopping…" disabled={loading || (!stopReady && !stop?.overridable)} onclick={onstop}>{entry.container ? "Stop container…" : "Stop…"}</Button>
        </div>
      </div>
      {#if stopReason}<p class="why"><Icon name={stop?.overridable || entry.protected ? "shield" : "lock"} size={13} /><span>{stopReason}{#if stop?.overridable} — review an explicit override to stop it.{/if}</span></p>
      {:else}<p class="scope">Stops the port owner; keeps this computer running.</p>{/if}
    </footer>
  {/if}
</aside>

<style>
  .pane { display: flex; flex-direction: column; min-height: 0; height: 100%; background: var(--surface); border-left: 1px solid var(--border); min-width: 0; container-type: inline-size; }
  .pane.drawer { border-left: 0; }
  .body { flex: 1; min-height: 0; min-width: 0; display: flex; flex-direction: column; }
  .body :global(.sa), .body :global(.vp), .body :global(.content) { min-width: 0; }
  .panel { outline: none; min-width: 0; padding: var(--sp-4) var(--sp-4) var(--sp-5); overflow-wrap: anywhere; }
  .actions { display: grid; gap: 8px; padding: 10px 14px 12px; border-top: 1px solid var(--border); background: var(--surface); flex: none; min-width: 0; }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; min-width: 0; }
  .utilities, .stop { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; min-width: 0; }
  .stop { margin-left: auto; }
  .why { display: flex; align-items: flex-start; gap: 6px; margin: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .why :global(svg) { flex: none; margin-top: 1px; }
  .scope { margin: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }

</style>
