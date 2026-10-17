<script lang="ts">
  // API keys an analyzer declares. A value goes straight to the OS keychain and is never shown
  // again; only that analyzer's runs get it. The field is cleared after saving.
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import * as api from "../../lib/api";

  let { id, name: analyzer, secrets, onchange }: {
    id: string;
    name: string;
    secrets: { name: string; stored: boolean }[];
    onchange?: () => void | Promise<void>;
  } = $props();
  let values = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);

  async function save(key: string) {
    const value = values[key] ?? "";
    if (!value) return;
    busy = key;
    error = null;
    try { await api.setAnalyzerSecret(id, key, value); values[key] = ""; await onchange?.(); }
    catch (e) { error = String(e); }
    finally { busy = null; }
  }
  async function remove(key: string) {
    busy = key;
    error = null;
    try { await api.deleteAnalyzerSecret(id, key); await onchange?.(); }
    catch (e) { error = String(e); }
    finally { busy = null; }
  }
</script>

{#if secrets.length}
  <div class="keys">
    <p class="meta">API keys go to your OS keychain. Only {analyzer} gets them, when it runs; holdmap never shows them again.</p>
    {#each secrets as s (s.name)}
      <div class="key">
        <label for="key-{id}-{s.name}"><code>{s.name}</code> <span class="st">{s.stored ? "stored" : "not stored"}</span></label>
        <input id="key-{id}-{s.name}" type="password" autocomplete="off" spellcheck="false" placeholder={s.stored ? "Replace the stored key" : "Paste the key"} bind:value={values[s.name]} onkeydown={(e) => { if (e.key === "Enter") void save(s.name); }} />
        <Button size="sm" variant="secondary" disabled={!values[s.name]} loading={busy === s.name} onclick={() => save(s.name)}>Save {s.name}</Button>
        {#if s.stored}<Button size="sm" variant="ghost" disabled={busy === s.name} onclick={() => remove(s.name)}>Delete {s.name}</Button>{/if}
      </div>
    {/each}
    {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  </div>
{/if}

<style>
  .keys { display: grid; gap: 6px; }
  .key { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  label { display: flex; gap: 6px; align-items: baseline; font-size: var(--fs-caption); }
  code { font-family: var(--mono); }
  .st { color: var(--muted); }
  input { flex: 1 1 180px; min-width: 0; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-caption); }
  input:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .meta { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
</style>
