<script lang="ts">
  import CodeBlock from "./CodeBlock.svelte";
  import Section from "./Section.svelte";
  import type { Explanation, PortEntry } from "../../lib/types";
  import { cliCommands } from "../../lib/detail";
  let { entry, explanation, oncopy }: { entry: PortEntry; explanation: Explanation | null; oncopy: (text: string, what: string) => void } = $props();
  const cmds = $derived(cliCommands(entry, explanation));
</script>

<Section title="From the terminal" icon="terminal">
  <p class="lead">The same answers and actions from a shell or a script.</p>
  <div class="list">{#each cmds as c (c.cmd)}<CodeBlock code={c.cmd} label={c.label} prompt {oncopy} />{/each}</div>
</Section>

<style>
  .lead { margin: -4px 0 0; color: var(--muted); font-size: var(--fs-body); line-height: var(--lh-body); }
  .list { display: grid; gap: 8px; }
</style>
