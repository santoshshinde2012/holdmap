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
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; font-size: var(--fs-caption); color: var(--muted); }
  li { display: inline-flex; align-items: center; gap: 6px; }
  li + li::before { content: ""; width: 18px; height: 1px; background: var(--border-strong); margin-right: 2px; }
  .dot { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; border: 1px solid var(--border-strong); font-size: var(--fs-label); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .now { color: var(--text); font-weight: var(--fw-semibold); }
  .now .dot { border-color: var(--accent); background: var(--accent); color: var(--accent-fg); }
  .done .dot { border-color: var(--accent); color: var(--accent); background: var(--accent-soft); }
</style>
