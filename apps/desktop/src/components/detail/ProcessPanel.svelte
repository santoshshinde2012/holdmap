<script lang="ts">
  import Facts, { type Fact } from "./Facts.svelte";
  import Section from "./Section.svelte";
  import CodeBlock from "./CodeBlock.svelte";
  import type { GraphNode, PortEntry } from "../../lib/types";
  import { command, humanBytes, tildify, uptime } from "../../lib/format";

  let { entry, node, oncopy }: { entry: PortEntry; node: GraphNode | null; oncopy: (text: string, what: string) => void } = $props();
  const p = $derived(entry.process);
  const tree = $derived(node && node.pids.length > 1 ? node : null);
  const procFacts = $derived.by((): Fact[] => p ? [
    { label: "Name", value: p.name, muted: `PID ${p.pid}` },
    { label: "User", value: entry.user, muted: entry.is_mine ? undefined : "not you" },
    { label: "Uptime", value: uptime(entry) },
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
{#if entry.project}
  <Section title="Project" icon="folder"><Facts facts={projFacts} {oncopy} /></Section>
{/if}
