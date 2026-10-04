<script lang="ts">
  import Icon from "../Icon.svelte";
  import Callout from "../ui/Callout.svelte";
  import RichText from "../ui/RichText.svelte";
  import Section from "./Section.svelte";
  import Sparkline from "../list/Sparkline.svelte";
  import type { Explanation, GraphNode, HttpInfo, PortDetails, PortEntry, ProcRef } from "../../lib/types";
  import { startedShort } from "../../lib/detail";
  import { describeStep, httpSummary, humanBytes, tildify, uptime } from "../../lib/format";
  import { tooltip } from "../../lib/tooltip";

  let { entry, explanation, http = null, loading, node, details = null, cpuTrend = [], memTrend = [] }: { entry: PortEntry; explanation: Explanation | null; http?: HttpInfo | null; loading: boolean; node: GraphNode | null; details?: PortDetails | null; cpuTrend?: number[]; memTrend?: number[] } = $props();
  const web = $derived(http && http.port === entry.port ? http : null);

  const plan = $derived(explanation?.plan ?? null);
  const blocked = $derived(plan?.blocked ?? null);
  const chain = $derived.by((): ProcRef[] => {
    const s = plan?.steps.find((x) => x.action === "signal_processes");
    return s && s.action === "signal_processes" ? s.processes : [];
  });
  const otherSteps = $derived(plan ? plan.steps.filter((s) => s.action !== "signal_processes") : []);
  const blockTitle: Record<string, string> = {
    needs_elevation: "Needs administrator rights",
    os_service: "Operating-system feature",
    nothing_to_stop: "Nothing to stop",
    protected: "Protected process",
  };
  const stats = $derived.by(() => {
    const p = entry.process;
    const out: { label: string; value: string; sub?: string; icon: string; wide?: boolean; trend?: number[]; range?: boolean }[] = [];
    if (uptime(entry)) out.push({ label: "Uptime", value: uptime(entry)!, sub: p?.start_time ? `since ${startedShort(p.start_time)}` : undefined, icon: "clock" });
    if (p?.memory_bytes) out.push({ label: "Memory", value: humanBytes(node && node.pids.length > 1 ? node.memory_bytes : p.memory_bytes), sub: node && node.pids.length > 1 ? `${node.pids.length} processes` : undefined, icon: "activity", trend: memTrend, range: true });
    if (p && p.cpu_percent !== undefined) out.push({ label: "CPU", value: `${(node?.cpu_percent ?? p.cpu_percent).toFixed(1)}%`, icon: "cpu", trend: cpuTrend });
    const c = details?.connections;
    // Shown from the start (as "–" until the lazy details land) so the tiles never reflow.
    if (entry.protocol === "tcp" && entry.state === "listen" && (p || entry.container)) {
      const peers = c ? c.peers.length + c.more_peers : 0;
      out.push({ label: "Connections", value: c ? String(c.established) : "–", sub: c ? (peers ? `${peers} peer${peers === 1 ? "" : "s"}` : "none open") : undefined, icon: "plug" });
    }
    if (entry.project) out.push({ label: "Project", value: entry.project.name, sub: tildify(entry.project.root), icon: "folder", wide: true });
    return out;
  });
</script>

{#if loading && !explanation}
  <div class="sk" aria-busy="true" aria-label="Loading explanation">
    <div class="shimmer" style="height:14px;width:92%"></div><div class="shimmer" style="height:14px;width:70%"></div><div class="shimmer" style="height:52px;margin-top:6px"></div>
  </div>
{:else if explanation}
  <div class="summary">
    <p class="headline selectable"><RichText text={explanation.headline} /></p>
    {#if blocked}
      <Callout tone="warn" icon={blocked.kind === "needs_elevation" ? "lock" : "shield"} title={blockTitle[blocked.kind]}><p class="selectable"><RichText text={blocked.message} /></p></Callout>
    {:else}
      <Callout tone="tip" title="Recommended"><p class="selectable"><RichText text={explanation.recommendation} /></p></Callout>
    {/if}
    {#if web}
      <p class="web selectable" class:bad={web.status >= 500}><Icon name="globe" size={12} /><span class="wl">HTTP</span><span class="wv">{httpSummary(web)}</span></p>
    {/if}
  </div>
{/if}

{#if stats.length}
  <div class="stats">
    {#each stats as s (s.label)}
      <div class="stat" class:wide={s.wide} use:tooltip={s.sub ? { text: s.sub, onlyIfTruncated: false } : null}>
        <span class="sl"><Icon name={s.icon} size={12} />{s.label}</span>
        <span class="sv">{s.value}{#if s.trend && s.trend.length > 1}<Sparkline values={s.trend} width={48} height={14} range={s.range} />{/if}</span>
        {#if s.sub}<span class="ss">{s.sub}</span>{/if}
      </div>
    {/each}
  </div>
{/if}

{#if plan && !blocked && plan.steps.length}
  <Section title={chain.length ? "What Stop will signal" : "What Stop will do"} icon="tree">
    {#snippet aside()}<span class="badge risk-{plan.risk}">{plan.risk} risk</span>{/snippet}
    <div class="plan">
      {#if chain.length}
        <ol class="chain">
          {#each chain as p, i}
            {@const holder = i === chain.length - 1}
            <li class:holder>
              <span class="node" aria-hidden="true"></span>
              <div class="pc">
                <div class="pl"><span class="pname">{p.name}</span><span class="pid">PID {p.pid}</span>{#if holder && chain.length > 1}<span class="holds">holds :{entry.port}</span>{/if}</div>
                <div class="pcmd" use:tooltip={{ text: p.command, mono: true, onlyIfTruncated: true }}>{p.command}</div>
              </div>
            </li>
          {/each}
        </ol>
      {/if}
      <ol class="steps">
        {#each chain.length ? otherSteps : plan.steps as s}<li><Icon name="check" size={12} /><span>{describeStep(s)}</span></li>{/each}
      </ol>
    </div>
    {#each plan.warnings as w}<Callout tone="warn" size="sm">{w}</Callout>{/each}
  </Section>
{/if}

<style>
  .sk { display: grid; gap: 10px; }
  .summary { display: grid; gap: var(--sp-3); }
  .headline { margin: 0; font-size: var(--fs-heading); letter-spacing: var(--ls-heading); line-height: var(--lh-heading); color: var(--text); font-weight: var(--fw-regular); text-wrap: pretty; }
  .web { margin: 0; display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); }
  .web :global(svg) { color: var(--ok); flex: none; }
  .web.bad :global(svg) { color: var(--danger); }
  .wl { font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); }
  .wv { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* One card of facts split by hairlines (not a grid of separate cards): less chrome, same data. */
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(118px, 1fr)); margin: var(--sp-5) 0 var(--sp-6); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  .stat { display: grid; align-content: start; gap: 2px; padding: 10px 12px; min-width: 0; box-shadow: -1px 0 0 var(--border), 0 -1px 0 var(--border); }
  .stat.wide { grid-column: 1 / -1; box-shadow: 0 -1px 0 var(--border); }
  .sl { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-label); line-height: var(--lh-label); color: var(--muted); font-weight: var(--fw-medium); text-transform: uppercase; letter-spacing: var(--ls-label); }
  .sv { display: flex; align-items: center; gap: 8px; font-size: var(--fs-heading); letter-spacing: var(--ls-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ss { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .plan { border: 1px solid var(--border); border-radius: var(--r-lg); padding: 12px 14px; display: grid; gap: 10px; }
  .chain { list-style: none; margin: 0; padding: 0; display: grid; }
  .chain li { display: flex; gap: 12px; position: relative; padding: 5px 0; min-width: 0; }
  .chain li:not(:last-child)::after { content: ""; position: absolute; left: 5px; top: 20px; bottom: -6px; width: 2px; background: var(--border-strong); border-radius: 2px; }
  .node { flex: none; width: 12px; height: 12px; margin-top: 3px; border-radius: 50%; border: 2px solid var(--border-strong); background: var(--surface); position: relative; z-index: 1; }
  .holder .node { border-color: var(--accent); background: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .pc { min-width: 0; flex: 1; }
  .pl { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; }
  .pname { font-weight: var(--fw-semibold); }
  .pid { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); }
  .holds { font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--accent); background: var(--accent-soft); padding: 1px 6px; border-radius: 999px; }
  .pcmd { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 2px; }
  .steps { list-style: none; margin: 0; padding: 10px 0 0; border-top: 1px dashed var(--border-strong); display: grid; gap: 6px; font-size: var(--fs-body); line-height: var(--lh-body); color: var(--text-2); }
  .chain:empty + .steps, .steps:first-child { border-top: 0; padding-top: 0; }
  .steps:empty { display: none; }
  .steps li { display: flex; gap: 8px; align-items: flex-start; }
  .steps :global(svg) { color: var(--ok); margin-top: 3px; flex: none; }
  .risk-low { color: var(--ok); background: var(--ok-soft); }
  .risk-medium { color: var(--warn); background: var(--warn-soft); }
  .risk-high { color: var(--danger); background: var(--danger-soft); }
</style>
