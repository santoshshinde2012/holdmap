<script lang="ts">
  // How one analyzer works: its pipeline from the inventory to where findings appear, with the
  // real permissions preview (reading the inventory starts nothing) and its last run.
  import Callout from "../../ui/Callout.svelte";
  import AnalyzerGraph from "./AnalyzerGraph.svelte";
  import * as api from "../../../lib/api";
  import { buildPipeline } from "../../../lib/analyzer-graph";
  import type { AnalyzerView, Preview } from "../../../lib/analyzers";
  import { insightsStore } from "../../../lib/insights-store.svelte";

  let { view, height = 360 }: { view: AnalyzerView; height?: number } = $props();
  let preview = $state<Preview | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<string | null>(null);

  $effect(() => {
    const id = view.id;
    preview = null;
    error = null;
    api.analyzerPreview(id).then((p) => { if (view.id === id) preview = p; }, (e) => { if (view.id === id) error = String(e); });
  });
  const result = $derived(insightsStore.insights?.results.find((r) => r.analyzer === view.id) ?? null);
  const graph = $derived(buildPipeline({ view, preview, result }));
</script>

<div class="pipeline" data-testid="analyzer-pipeline">
  <AnalyzerGraph {graph} {height} bind:selected label={`How ${view.name} works`} />
  {#if error}<Callout tone="warn" size="sm">The inventory couldn't be read for this graph: {error}</Callout>{/if}
</div>

<style>
  .pipeline { display: grid; gap: var(--sp-2); min-width: 0; }
</style>
