<script lang="ts">
  // Monospace command/code with a labelled copy button; long lines scroll, never overflow.
  import Button from "../ui/Button.svelte";
  let { code, what = "Command", prompt = false, label, oncopy }: { code: string; what?: string; prompt?: boolean; label?: string; oncopy: (text: string, what: string) => void } = $props();
  let copied = $state(false);
  let t: ReturnType<typeof setTimeout> | undefined;
  function copy() { oncopy(code, what); copied = true; clearTimeout(t); t = setTimeout(() => (copied = false), 1400); }
</script>

<div class="cb">
  {#if label}<div class="cl">{label}</div>{/if}
  <div class="row">
    <pre class="selectable">{#if prompt}<span class="pr" aria-hidden="true">$ </span>{/if}<code>{code}</code></pre>
    <Button size="xs" variant="ghost" icon={copied ? "check" : "copy"} aria-label={copied ? `${what} copied` : `Copy ${what.toLowerCase()}`} onclick={copy}>{copied ? "Copied" : "Copy"}</Button>
  </div>
</div>

<style>
  .cb { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface-2); overflow: hidden; }
  .cl { font-size: var(--fs-label); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); font-weight: var(--fw-medium); color: var(--muted); padding: 7px 12px 0; }
  .row { display: flex; align-items: flex-start; gap: 6px; padding: 6px 6px 6px 12px; }
  pre { flex: 1; min-width: 0; margin: 0; padding: 3px 0; font-family: var(--mono); font-size: var(--fs-mono); line-height: var(--lh-mono); color: var(--text); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 110px; overflow-y: auto; }
  code { font-size: inherit; }
  .pr { color: var(--faint); }
</style>
