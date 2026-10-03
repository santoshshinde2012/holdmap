<script lang="ts">
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import Section from "./Section.svelte";
  import type { Cluster, Graph, GraphNode } from "../../lib/types";
  import { CLUSTER_LABEL, edgeLabel } from "../../lib/graph";

  let { graph, node, cluster, deps, users, onstopcluster, onselectnode }: {
    graph: Graph; node: GraphNode; cluster: Cluster | null; deps: GraphNode[]; users: GraphNode[];
    onstopcluster?: (name: string) => void; onselectnode?: (n: GraphNode) => void;
  } = $props();
  const edge = (from: string, to: string) => graph.edges.find((e) => e.from === from && e.to === to);
  const ports = (n: GraphNode) => n.ports.map((p) => `:${p.port}`).join(" ");
</script>

{#if cluster}
  <div class="cluster">
    <span class="ct" aria-hidden="true"><Icon name="layers" size={16} /></span>
    <div class="cn"><b>{cluster.name}</b><span>{CLUSTER_LABEL[cluster.kind]}{cluster.detail ? ` · ${cluster.detail}` : ""} · {cluster.nodes.length} services</span></div>
    {#if onstopcluster}<Button size="sm" variant="danger-outline" icon="stop" onclick={() => onstopcluster?.(cluster.name)} tip={`Stop every service in ${cluster.name}, dependents first`}>Stop cluster</Button>{/if}
  </div>
{/if}

{#snippet rel(list: GraphNode[], dir: "out" | "in")}
  <ul class="rel">
    {#each list as d (d.id)}
      {@const e = dir === "out" ? edge(node.id, d.id) : edge(d.id, node.id)}
      <li>
        <button type="button" class="relbtn" onclick={() => onselectnode?.(d)} aria-label="Select {d.label}">
          <span class="arrow {dir}" aria-hidden="true"><Icon name={dir === "out" ? "arrow" : "arrow-left"} size={12} /></span>
          <span class="rn">{d.label}</span>
          <span class="rp">{ports(d)}</span>
          {#if e}<span class="rt">{edgeLabel(e)}</span>{/if}
          <Icon name="chevron" size={13} />
        </button>
      </li>
    {/each}
  </ul>
{/snippet}

{#if deps.length}
  <Section title="Depends on" icon="arrow">
    {#snippet aside()}<span class="hint">{deps.length} service{deps.length === 1 ? "" : "s"} this one calls</span>{/snippet}
    {@render rel(deps, "out")}
  </Section>
{/if}
{#if users.length}
  <Section title="Used by" icon="arrow-left">
    {#snippet aside()}<span class="hint">{users.length} caller{users.length === 1 ? "" : "s"}</span>{/snippet}
    {@render rel(users, "in")}
    <Callout tone="warn" size="sm">{users.length === 1 ? `${users[0].label} depends` : `${users.length} services depend`} on this — stopping it will break {users.length === 1 ? "it" : "them"}.</Callout>
  </Section>
{/if}
{#if !deps.length && !users.length}
  <p class="none">No live connections to other local services right now.</p>
{/if}

<style>
  .cluster { display: flex; align-items: center; gap: 12px; padding: 12px; border-radius: var(--r-lg); border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent); background: var(--accent-softer); margin-bottom: var(--sp-6); }
  .ct { width: 34px; height: 34px; border-radius: var(--r-md); display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); flex: none; }
  .cn { flex: 1; min-width: 0; display: grid; gap: 1px; }
  .cn b { font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cn span { font-size: var(--fs-xs); color: var(--muted); }
  .hint { font-size: var(--fs-xs); color: var(--muted); }
  .rel { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .rel li + li { border-top: 1px solid var(--border); }
  .relbtn { width: 100%; display: flex; align-items: center; gap: 10px; min-height: 40px; padding: 6px 10px 6px 12px; border: 0; background: transparent; font: inherit; color: var(--text); cursor: pointer; text-align: left; }
  .relbtn:hover { background: var(--row-hover); }
  .relbtn:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .relbtn > :global(svg:last-child) { color: var(--faint); }
  .arrow { width: 22px; height: 22px; border-radius: var(--r-sm); display: grid; place-items: center; flex: none; }
  .arrow.out { color: var(--tone-green); background: var(--tone-green-bg); }
  .arrow.in { color: var(--tone-amber); background: var(--tone-amber-bg); }
  .rn { font-weight: 600; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rp { font-family: var(--mono); font-size: 11.5px; color: var(--muted); }
  .rt { margin-left: auto; font-family: var(--mono); font-size: 11px; color: var(--muted); white-space: nowrap; }
  .none { color: var(--muted); margin: 0; }
</style>
