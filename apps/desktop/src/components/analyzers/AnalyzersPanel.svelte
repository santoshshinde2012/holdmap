<script lang="ts">
  // Settings → Analyzers: every analyzer is off until its permissions sheet is approved.
  // Run now, Turn off and the last run sit beside each one.
  import { onMount } from "svelte";
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import CopyValue from "../ui/CopyValue.svelte";
  import PermissionsSheet from "./PermissionsSheet.svelte";
  import * as api from "../../lib/api";
  import { STATE_LABEL, ago, countsText, total, type AnalyzerView, type Preview } from "../../lib/analyzers";
  import { insightsStore } from "../../lib/insights-store.svelte";

  let { demo = false }: { demo?: boolean } = $props();
  let preview = $state<Preview | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string | null>>({});
  let sheetError = $state<string | null>(null);

  onMount(() => { void insightsStore.refresh(); });

  async function act(key: string, f: () => Promise<unknown>) {
    if (busy[key]) return;
    busy[key] = true;
    errors[key] = null;
    try { await f(); await insightsStore.refresh(); }
    catch (e) { errors[key] = String(e); }
    finally { busy[key] = false; }
  }
  const review = (a: AnalyzerView) => act(a.id, async () => { sheetError = null; preview = await api.analyzerPreview(a.id); });
  async function approve() {
    if (!preview) return;
    const id = preview.analyzer.id;
    busy.sheet = true;
    sheetError = null;
    try { await api.approveAnalyzer(id); preview = null; await insightsStore.refresh(); }
    catch (e) { sheetError = String(e); }
    finally { busy.sheet = false; }
  }
  const lastRun = (a: AnalyzerView) => {
    const r = a.last_run;
    if (!r) return "Not run yet";
    const open = total(r.open);
    return `Last run ${ago(r.run_at_ms)} · ${r.subjects} ${r.subjects === 1 ? "item" : "items"} · ${open ? `${countsText(r.open)} open` : "no open findings"}${r.status !== "complete" ? ` · ${r.status}` : ""}`;
  };
</script>

{#if preview}
  <PermissionsSheet {preview} busy={busy.sheet} error={sheetError} onapprove={approve} oncancel={() => (preview = null)} />
{:else}
  <p class="intro">Analyzers are scanners you choose. They look at your agents' skills and MCP packages and report findings on Home and on each agent. Each one is off until you review exactly what it gets.</p>
  {#if demo}<Callout tone="info" size="sm">Browser demo: results are sample data; nothing is installed, run or sent.</Callout>{/if}
  {#if insightsStore.error}<Callout tone="danger" size="sm">{insightsStore.error}</Callout>{/if}
  <ul class="list">
    {#each insightsStore.analyzers as a (a.id)}
      <li class="st-{a.state}">
        <div class="top">
          <span class="name">{a.name}</span>
          <span class="pub">{a.publisher}</span>
          <span class="state" data-state={a.state}>{STATE_LABEL[a.state]}</span>
        </div>
        <p class="sum">{a.summary}</p>
        {#if a.state === "approved"}<p class="meta">{lastRun(a)}</p>{/if}
        {#if a.state === "changed"}<p class="meta warn">{a.changes.join("; ")}</p>{/if}
        {#if a.state === "not_installed" && a.install}
          <div class="install"><span class="meta">Install it yourself, then come back:</span><CopyValue value={a.install} mono wrap what="Install command" oncopy={(t) => navigator.clipboard?.writeText(t)} /></div>
        {/if}
        <div class="acts">
          {#if a.state === "approved"}
            <Button size="sm" variant="primary" icon="play" loading={busy[`run:${a.id}`]} onclick={() => act(`run:${a.id}`, () => api.runAnalyzer(a.id))}>Run now</Button>
            <Button size="sm" variant="ghost" onclick={() => review(a)} loading={busy[a.id]}>Permissions</Button>
            <Button size="sm" variant="danger-outline" loading={busy[`off:${a.id}`]} onclick={() => act(`off:${a.id}`, () => api.revokeAnalyzer(a.id))}>Turn off</Button>
          {:else if a.state !== "not_installed"}
            <Button size="sm" variant={a.state === "changed" ? "primary" : "secondary"} icon="shield" loading={busy[a.id]} onclick={() => review(a)}>{a.state === "changed" ? "Review again…" : "Review and turn on…"}</Button>
          {/if}
        </div>
        {#each [errors[a.id], errors[`run:${a.id}`], errors[`off:${a.id}`]].filter(Boolean) as e}<Callout tone="danger" size="sm">{e}</Callout>{/each}
      </li>
    {/each}
  </ul>
  <p class="foot">holdmap never starts MCP servers for analyzers. Runs get a copy of each folder, a cleared environment and a time limit. Only short findings are kept.</p>
{/if}

<style>
  .intro, .foot { margin: 0 0 var(--sp-3); color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .foot { margin-top: var(--sp-3); color: var(--muted); font-size: var(--fs-caption); }
  .list { list-style: none; margin: var(--sp-3) 0 0; padding: 0; display: grid; gap: var(--sp-2); }
  li { display: grid; gap: 4px; padding: 10px var(--sp-3); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .top { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px var(--sp-2); }
  .name { font-weight: var(--fw-semibold); }
  .pub { color: var(--muted); font-size: var(--fs-caption); }
  .state { margin-left: auto; font-size: var(--fs-caption); padding: 1px 8px; border-radius: var(--r-full); border: 1px solid var(--border); color: var(--muted); }
  .state[data-state="approved"] { color: var(--ok-text, var(--ok)); border-color: var(--ok); }
  .state[data-state="changed"] { color: var(--warn-text, var(--warn)); border-color: var(--warn); }
  .sum, .meta { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); }
  .meta { font-size: var(--fs-caption); color: var(--muted); }
  .meta.warn { color: var(--warn-text, var(--warn)); }
  .install { display: grid; gap: 4px; }
  .acts { display: flex; flex-wrap: wrap; gap: var(--sp-2); margin-top: 2px; }
  .acts:empty { display: none; }
</style>
