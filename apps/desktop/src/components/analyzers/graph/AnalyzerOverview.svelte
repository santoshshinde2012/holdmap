<script lang="ts">
  // Which analyzer covers which agent, and what it found: analyzers on the left, agents on the
  // right, one edge per agent an analyzer's last run looked at. Selecting an analyzer offers
  // its pipeline graph.
  import AnalyzerGraph from "./AnalyzerGraph.svelte";
  import { buildOverview, graphSpan, type GraphNode } from "../../../lib/analyzer-graph";
  import { insightsStore } from "../../../lib/insights-store.svelte";
  import type { AgentsReport } from "../../../lib/types";

  let { report, height = 420, onpipeline }: { report: AgentsReport | null; height?: number; onpipeline?: (id: string) => void } = $props();
  let selected = $state<string | null>(null);
  const graph = $derived(buildOverview(insightsStore.analyzers, insightsStore.insights, report));
  // Tall enough that fit-to-view keeps cards at full size. Past eight agents the canvas stops
  // growing and the diagram scales down instead of pushing the rest of the page off screen.
  const fitted = $derived(graph.nodes.length ? Math.min(760, Math.max(height, Math.ceil(graphSpan(graph).height + 88))) : height);
  const open = (n: GraphNode) => (n.kind === "analyzer" && onpipeline ? { label: `How ${n.title} works`, run: () => onpipeline(n.id.slice("analyzer:".length)) } : null);
</script>

<div data-testid="analyzer-overview">
  {#if graph.nodes.length}
    <AnalyzerGraph {graph} height={fitted} bind:selected label="Analyzers and the agents they cover" onopen={open} />
  {:else}
    <p class="empty">No analyzers or agents to show yet.</p>
  {/if}
</div>

<style>
  .empty { margin: 0; color: var(--muted); font-size: var(--fs-body-sm); }
</style>
