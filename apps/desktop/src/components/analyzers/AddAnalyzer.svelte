<script lang="ts">
  // Register your own analyzer (a local CLI or MCP scanner) by pasting its manifest. It is
  // validated by holdmap and stays off until its permissions sheet is approved.
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import * as api from "../../lib/api";

  let { onadded }: { onadded?: (id: string) => void } = $props();
  let open = $state(false);
  let text = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function add() {
    if (!text.trim() || busy) return;
    busy = true;
    error = null;
    try {
      const view = await api.addAnalyzer(text);
      text = "";
      open = false;
      onadded?.(view.id);
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<div class="add">
  {#if !open}
    <Button size="sm" variant="ghost" icon="plus" onclick={() => (open = true)}>Add your own analyzer…</Button>
  {:else}
    <label for="analyzer-manifest">Analyzer manifest (TOML)</label>
    <p class="meta">A local command-line or MCP scanner. holdmap checks the manifest and keeps it off until you review and turn it on. An MCP analyzer's server starts only when you run it.</p>
    <textarea id="analyzer-manifest" rows="8" spellcheck="false" bind:value={text} placeholder={'schema = 1\nid = "example.scanner"\n…'}></textarea>
    <div class="acts">
      <Button size="sm" variant="primary" loading={busy} disabled={!text.trim()} onclick={add}>Add analyzer</Button>
      <Button size="sm" variant="ghost" onclick={() => { open = false; error = null; }}>Cancel</Button>
    </div>
    {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  {/if}
</div>

<style>
  .add { display: grid; gap: var(--sp-2); margin-top: var(--sp-3); }
  label { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .meta { margin: 0; font-size: var(--fs-caption); color: var(--muted); }
  textarea { font-family: var(--mono); font-size: var(--fs-caption); padding: var(--sp-2); border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--input-bg, var(--surface)); color: var(--text); resize: vertical; }
  .acts { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
</style>
