<script lang="ts">
  // The signed analyzer catalog: which version is in use, a user-initiated check for a newer
  // one, and the diff to review before it applies. Nothing is downloaded until "Check" is pressed.
  import { onMount } from "svelte";
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import * as api from "../../lib/api";
  import type { CatalogStatus } from "../../lib/analyzers";

  let { onchange }: { onchange?: () => void } = $props();
  let status = $state<CatalogStatus | null>(null);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let note = $state<string | null>(null);

  async function refresh() { status = await api.analyzerCatalog(); }
  onMount(() => { void refresh().catch((e) => (error = String(e))); });

  async function act(what: string, f: () => Promise<unknown>, done?: string) {
    if (busy) return;
    busy = what;
    error = null;
    note = null;
    try { await f(); await refresh(); note = done ?? null; onchange?.(); }
    catch (e) { error = String(e); }
    finally { busy = null; }
  }
  const empty = (v: string) => v || "(none)";
</script>

{#if status}
  <section class="cat" aria-label="Analyzer catalog">
    <div class="row">
      <p class="meta">Catalog version {status.version} · {status.published} · {status.accepted ? "update you accepted" : "built into holdmap"} · signed by holdmap</p>
      {#if !status.pending}
        <Button size="sm" variant="ghost" icon="refresh" loading={busy === "fetch"} onclick={() => act("fetch", api.fetchAnalyzerCatalog)}>Check for a catalog update</Button>
      {/if}
    </div>
    {#if status.pending}
      {@const d = status.pending}
      <div class="diff" role="group" aria-label="Catalog update to review">
        <p class="head">Signed update: version {d.from_version} → {d.to_version} ({d.published}). Review it before it applies.</p>
        <ul>
          {#each d.added as a}<li><span class="tag add">New</span> {a}</li>{/each}
          {#each d.removed as r}<li><span class="tag del">Removed</span> {r}</li>{/each}
          {#each d.changed as c}
            <li><span class="tag chg">Changed</span> <code>{c.id}</code>
              <ul class="fields">{#each c.fields as f}<li><code>{f.field}</code>: {empty(f.before)} → {empty(f.after)}</li>{/each}</ul>
            </li>
          {/each}
          {#if !d.added.length && !d.removed.length && !d.changed.length}<li>No analyzer changes.</li>{/if}
        </ul>
        <p class="meta">Analyzers you turned on whose manifest changes will ask for approval again.</p>
        <div class="acts">
          <Button size="sm" variant="primary" loading={busy === "accept"} onclick={() => act("accept", api.acceptAnalyzerCatalog, "Catalog updated.")}>Accept update</Button>
          <Button size="sm" variant="ghost" loading={busy === "reject"} onclick={() => act("reject", api.rejectAnalyzerCatalog)}>Reject</Button>
        </div>
      </div>
    {/if}
    {#if note}<p class="meta" role="status">{note}</p>{/if}
    {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  </section>
{/if}

<style>
  .cat { display: grid; gap: var(--sp-2); margin: var(--sp-3) 0 0; }
  .row { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: var(--sp-2); }
  .meta { margin: 0; font-size: var(--fs-caption); color: var(--muted); }
  .diff { display: grid; gap: var(--sp-2); padding: 10px var(--sp-3); border: 1px solid var(--warn); border-radius: var(--r-lg); background: var(--surface); }
  .head { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  ul { margin: 0; padding-left: var(--sp-4); display: grid; gap: 4px; font-size: var(--fs-body-sm); }
  .fields { padding-left: var(--sp-4); color: var(--text-2); }
  code { font-family: var(--mono); font-size: var(--fs-caption); }
  .tag { font-size: var(--fs-caption); padding: 0 6px; border-radius: var(--r-full); border: 1px solid var(--border); }
  .tag.add { border-color: var(--ok); }
  .tag.del { border-color: var(--danger, var(--warn)); }
  .tag.chg { border-color: var(--warn); }
  .acts { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
</style>
