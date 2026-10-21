<script lang="ts">
  // Calm empty states: first setup, all clear, or nothing matches the filters.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  const TEXT = { setup: ["No analyzer is on", "Pick one to scan your agents' skills and MCP packages. Each asks first and runs sandboxed."], clear: ["No open findings", "Everything scanned looks fine."], nomatch: ["No findings match", ""] };
  let { kind, onprimary, onclear }: { kind: "setup" | "clear" | "nomatch"; onprimary?: () => void; onclear?: () => void } = $props();
</script>

<div class="empty k-{kind}">
  <span class="ic"><Icon name={{ setup: "shield", clear: "check", nomatch: "filter" }[kind]} size={18} /></span>
  <p>{TEXT[kind][0]}</p>
  {#if TEXT[kind][1]}<span>{TEXT[kind][1]}</span>{/if}
  {#if kind === "setup" && onprimary}<Button size="sm" variant="primary" icon="plus" onclick={onprimary}>Choose an analyzer</Button>
  {:else if kind === "nomatch" && onclear}<Button size="sm" variant="ghost" onclick={onclear}>Clear filters</Button>{/if}
</div>

<style>
  .empty { display: grid; justify-items: center; gap: 6px; padding: 40px 16px; border: 1px dashed var(--border-strong); border-radius: var(--r-lg); text-align: center; color: var(--muted); font-size: var(--fs-body-sm); }
  .empty > span:not(.ic) { max-width: 360px; }
  .ic { display: grid; place-items: center; width: 36px; height: 36px; margin-bottom: 4px; border-radius: 50%; background: var(--accent-soft); color: var(--accent); }
  .k-clear .ic { background: var(--ok-soft); color: var(--ok); }
  p { margin: 0; color: var(--text); font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .empty :global(.ui-btn) { margin-top: 8px; }
</style>
