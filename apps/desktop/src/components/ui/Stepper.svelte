<script lang="ts">
  // A small numbered step indicator: done steps get a check, the current one is marked for
  // assistive tech with aria-current="step".
  import Icon from "../Icon.svelte";
  let { steps, current, label = "Steps" }: { steps: { id: string; label: string }[]; current: string; label?: string } = $props();
  const at = $derived(Math.max(0, steps.findIndex((s) => s.id === current)));
</script>

<ol class="steps" aria-label={label}>
  {#each steps as s, i (s.id)}
    <li class:done={i < at} class:now={i === at} aria-current={i === at ? "step" : undefined}>
      <span class="dot" aria-hidden="true">{#if i < at}<Icon name="check" size={11} />{:else}{i + 1}{/if}</span>{s.label}
    </li>
  {/each}
</ol>

<style>
  .steps { list-style: none; margin: 0; padding: 0; display: flex; align-items: center; font-size: var(--fs-caption); color: var(--muted); }
  li { display: inline-flex; align-items: center; gap: 8px; white-space: nowrap; }
  li + li::before { content: ""; width: 28px; height: 2px; margin: 0 10px; border-radius: 1px; background: var(--border); }
  li.done + li::before, li.done + li.now::before { background: var(--accent); }
  .dot { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; border: 1px solid var(--border-strong); background: var(--surface); font-size: var(--fs-label); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; transition: background var(--dur-2) var(--ease), border-color var(--dur-2) var(--ease); }
  .now { color: var(--text); font-weight: var(--fw-semibold); }
  .now .dot { border-color: var(--accent); background: var(--accent); color: var(--accent-fg); box-shadow: 0 0 0 4px var(--accent-soft); }
  .done { color: var(--text-2); }
  .done .dot { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
</style>
