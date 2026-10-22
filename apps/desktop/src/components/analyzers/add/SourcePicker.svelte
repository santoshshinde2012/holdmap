<script lang="ts" module>
  export interface SourceOption { value: string; label: string; icon: string; desc: string }
</script>

<script lang="ts">
  // Where an analyzer comes from, as large selectable cards (a radiogroup: arrows move, Space picks).
  import Icon from "../../Icon.svelte";
  let { options, value = $bindable(), label }: { options: SourceOption[]; value: string; label: string } = $props();
  let refs = $state<HTMLButtonElement[]>([]);
  function key(e: KeyboardEvent, i: number) {
    const d = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = (i + d + options.length) % options.length;
    value = options[n].value;
    refs[n]?.focus();
  }
</script>

<div class="sources" role="radiogroup" aria-label={label}>
  {#each options as o, i (o.value)}
    <button bind:this={refs[i]} type="button" role="radio" class="src" aria-checked={value === o.value} aria-label={o.label} aria-describedby="src-{o.value}"
      tabindex={value === o.value ? 0 : -1} onclick={() => (value = o.value)} onkeydown={(e) => key(e, i)}>
      <span class="ic" aria-hidden="true"><Icon name={o.icon} size={16} /></span>
      <span class="t">{o.label}</span>
      <span class="d" id="src-{o.value}">{o.desc}</span>
      <span class="tick" aria-hidden="true"><Icon name="check" size={11} /></span>
    </button>
  {/each}
</div>

<style>
  .sources { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 8px; }
  .src { position: relative; display: grid; grid-template-columns: auto 1fr; grid-template-areas: "ic t" "d d"; align-items: center; gap: 6px 10px; padding: 12px; min-height: 84px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; transition: border-color var(--dur-1) var(--ease), background var(--dur-1) var(--ease), box-shadow var(--dur-1) var(--ease); }
  .src:hover { border-color: var(--border-strong); background: var(--surface-2); }
  .src[aria-checked="true"] { border-color: var(--accent); background: var(--accent-softer); box-shadow: 0 0 0 1px var(--accent); }
  .src:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .ic { grid-area: ic; display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--r-md); background: var(--surface-3); color: var(--text-2); }
  .src[aria-checked="true"] .ic { background: var(--accent-soft); color: var(--accent); }
  .t { grid-area: t; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .d { grid-area: d; align-self: start; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .tick { position: absolute; top: 10px; right: 10px; display: grid; place-items: center; width: 16px; height: 16px; border-radius: 50%; background: var(--accent); color: var(--accent-fg); opacity: 0; transform: scale(0.6); transition: opacity var(--dur-1) var(--ease), transform var(--dur-1) var(--ease); }
  .src[aria-checked="true"] .tick { opacity: 1; transform: none; }
</style>
