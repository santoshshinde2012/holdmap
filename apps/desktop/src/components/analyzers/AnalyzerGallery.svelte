<script lang="ts">
  // Analyzer cards: the ones on, then the ones you can add. Run is the only action that acts here;
  // Enable, Review and Install open the permissions sheet in Settings, so consent and sandboxing
  // are unchanged and nothing is on by default.
  import { onMount } from "svelte";
  import Icon from "../Icon.svelte";
  import AnalyzerCard from "../security/AnalyzerCard.svelte";
  import * as api from "../../lib/api";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { analyzerCards } from "../../lib/security-view";
  import type { PresetView } from "../../lib/analyzer-presets";

  let { onsettings, onpipeline }: { onsettings?: () => void; onpipeline?: (id: string) => void } = $props();
  let presets = $state<PresetView[]>([]);
  onMount(() => { api.analyzerPresets().then((p) => (presets = p), () => (presets = [])); });
  const cards = $derived(analyzerCards(insightsStore.analyzers, presets));
  const enabled = $derived(cards.filter((c) => c.status === "on" || c.status === "changed"));
  const available = $derived(cards.filter((c) => c.status !== "on" && c.status !== "changed"));
  const configured = (id: string) => insightsStore.analyzers.some((a) => a.id === id);
  const running = $derived(securityRuns.steps.find((s) => s.phase === "running")?.id ?? null);

  function act(id: string, action: string) {
    if (action === "run") { const a = insightsStore.analyzers.find((x) => x.id === id); if (a) void securityRuns.start([a]); }
    else onsettings?.();
  }
</script>

<div class="gallery">
  <p class="trust"><Icon name="lock" size={12} />Off by default · asks before it runs · sandboxed</p>
  {#each [{ key: "on", title: "Enabled", list: enabled }, { key: "off", title: "Available", list: available }] as sec (sec.key)}
    {#if sec.list.length || sec.key === "off"}
      <section aria-labelledby="gal-{sec.key}">
        <h3 id="gal-{sec.key}">{sec.title}<span>{sec.list.length}</span></h3>
        <ul>
          {#each sec.list as card (card.id)}
            <AnalyzerCard {card} busy={running === card.id} disabled={card.action === "run" && securityRuns.busy && running !== card.id}
              onaction={() => act(card.id, card.action)}
              onpipeline={onpipeline && configured(card.id) ? () => onpipeline(card.id) : undefined} />
          {/each}
          {#if sec.key === "off" && onsettings}
            <li class="add"><button type="button" onclick={onsettings}><Icon name="plus" size={16} /><b>Add your own</b><small>CLI or MCP server</small></button></li>
          {/if}
        </ul>
      </section>
    {/if}
  {/each}
</div>

<style>
  .gallery { display: grid; align-content: start; gap: 20px; }
  .trust { display: inline-flex; align-items: center; gap: 6px; margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  section { display: grid; gap: 10px; }
  h3 { display: flex; align-items: center; gap: 8px; margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  h3 span { color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 260px), 1fr)); gap: 12px; }
  .add button { display: grid; place-content: center; justify-items: center; gap: 4px; width: 100%; height: 100%; min-height: 150px; border: 1px dashed var(--border-strong); border-radius: var(--r-lg); background: none; color: var(--text-2); font: inherit; cursor: pointer; }
  .add button:hover { background: var(--surface-2); color: var(--text); }
  .add button:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .add b { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .add small { color: var(--muted); font-size: var(--fs-caption); }
</style>
