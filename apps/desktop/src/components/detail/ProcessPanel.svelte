<script lang="ts">
  import Facts, { type Fact } from "./Facts.svelte";
  import Section from "./Section.svelte";
  import CodeBlock from "./CodeBlock.svelte";
  import type { GraphNode, PortDetails, PortEntry, TreeProcess } from "../../lib/types";
  import { command, humanBytes, tildify, uptime } from "../../lib/format";
  import { startedAt } from "../../lib/detail";
  import { tooltip } from "../../lib/tooltip";

  let { entry, node, oncopy, details = null }: { entry: PortEntry; node: GraphNode | null; oncopy: (text: string, what: string) => void; details?: PortDetails | null } = $props();
  const ptree = $derived(details?.tree ?? null);
  /** Parents outermost first, then the listener, then its descendants: one indented list. */
  const rows = $derived.by((): (TreeProcess & { level: number; self?: boolean; up?: boolean })[] => {
    if (!ptree) return [];
    const up = ptree.ancestors.map((a, i) => ({ ...a, level: i, up: true }));
    const base = up.length;
    return [...up, { ...ptree.process, level: base, self: true }, ...ptree.children.map((c) => ({ ...c, level: base + c.depth }))];
  });
  const p = $derived(entry.process);
  const tree = $derived(node && node.pids.length > 1 ? node : null);
  const procFacts = $derived.by((): Fact[] => p ? [
    { label: "Name", value: p.name, muted: `PID ${p.pid}` },
    { label: "User", value: entry.user, muted: entry.is_mine ? undefined : "not you" },
    { label: "Uptime", value: uptime(entry), muted: p.start_time ? `since ${startedAt(p.start_time)}` : undefined },
    { label: "Memory", value: p.memory_bytes ? humanBytes(p.memory_bytes) : null, muted: tree ? `tree ${humanBytes(tree.memory_bytes)}` : undefined },
    { label: "CPU", value: p.cpu_percent !== undefined ? `${p.cpu_percent.toFixed(1)}%` : null, muted: tree ? `${tree.pids.length} processes · ${tree.cpu_percent.toFixed(1)}%` : undefined },
    { label: "Directory", value: p.cwd, display: p.cwd ? tildify(p.cwd) : undefined, mono: true, copy: true },
    { label: "Executable", value: p.exe, display: p.exe ? tildify(p.exe) : undefined, mono: true, copy: true },
  ] : []);
  const projFacts = $derived.by((): Fact[] => entry.project ? [
    { label: "Name", value: entry.project.name },
    { label: "Path", value: entry.project.root, display: tildify(entry.project.root), mono: true, copy: true },
    { label: "Detected", value: entry.project.kind, mono: true },
    { label: "Branch", value: entry.project.git_branch, mono: true },
    { label: "Workspace", value: entry.project.workspace?.name, muted: entry.project.workspace?.kind },
  ] : []);
  const ctrFacts = $derived.by((): Fact[] => entry.container ? [
    { label: "Name", value: entry.container.name, copy: true },
    { label: "Image", value: entry.container.image, mono: true, copy: true },
    { label: "Runtime", value: entry.container.runtime },
    { label: "Mapping", value: `${entry.port} → ${entry.container.private_port}`, mono: true },
    { label: "Compose", value: entry.container.compose_project ? `${entry.container.compose_project}/${entry.container.compose_service}` : null, mono: true },
    { label: "ID", value: entry.container.id, mono: true, copy: true },
  ] : []);
</script>

{#if entry.container}
  <Section title="Container" icon="box"><Facts facts={ctrFacts} {oncopy} /></Section>
{/if}
{#if p}
  <Section title="Process" icon="terminal">
    <Facts facts={procFacts} {oncopy} />
    <CodeBlock code={command(entry)} what="Command" {oncopy} />
  </Section>
{/if}
{#if rows.length > 1}
  <Section title="Process tree" icon="tree">
    {#snippet aside()}<span class="cnt">{ptree!.ancestors.length} parent{ptree!.ancestors.length === 1 ? "" : "s"} · {ptree!.children.length + ptree!.more_children} child{ptree!.children.length + ptree!.more_children === 1 ? "" : "ren"}</span>{/snippet}
    <ul class="tree" aria-label="Process tree">
      {#each rows as r (r.pid)}
        <li class:self={r.self} class:up={r.up} style="--lvl:{Math.min(r.level, 8)}" use:tooltip={{ text: r.command, onlyIfTruncated: false }}>
          <span class="tn">{r.name}</span><span class="tp">{r.pid}</span>{#if r.self}<span class="here">listening</span>{/if}
          {#if r.memory_bytes && !r.up}<span class="tm">{humanBytes(r.memory_bytes)}</span>{/if}
        </li>
      {/each}
    </ul>
    {#if ptree!.more_children}<p class="more">and {ptree!.more_children} more</p>{/if}
  </Section>
{/if}
{#if entry.project}
  <Section title="Project" icon="folder"><Facts facts={projFacts} {oncopy} /></Section>
{/if}

<style>
  .cnt { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .tree { list-style: none; margin: 0; padding: 6px 0; border: 1px solid var(--border); border-radius: var(--r-lg); font-size: var(--fs-body); line-height: var(--lh-body); }
  .tree li { display: flex; align-items: baseline; gap: 8px; padding: 3px 12px 3px calc(12px + var(--lvl) * 14px); min-width: 0; position: relative; }
  .tree li:not(:first-child)::before { content: "└"; position: absolute; left: calc(var(--lvl) * 14px - 2px); color: var(--faint); font-family: var(--mono); font-size: var(--fs-mono-sm); }
  .tree li.up { color: var(--muted); }
  .tree li.self .tn { font-weight: var(--fw-semibold); color: var(--text); }
  .tn { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .tp, .tm { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); white-space: nowrap; }
  .tm { margin-left: auto; }
  .here { font-size: var(--fs-label); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); font-weight: var(--fw-medium); color: var(--accent); }
  .more { margin: 0; color: var(--muted); font-size: var(--fs-body-sm); }
</style>
