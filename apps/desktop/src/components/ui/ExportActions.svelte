<script lang="ts">
  // Copy a summary and save JSON / SARIF files. Each action builds its text only when used, so
  // opening a view never prepares a report; the outcome is announced beside the buttons.
  import Button from "./Button.svelte";
  import { copyGeneratedText } from "../../lib/generated-clipboard";
  import { saveReport } from "../../lib/report-export";

  export interface ExportFile { label: string; filename: string; build: () => unknown }
  let { copy, files, disabled = false, label = "Export" }: {
    /** The text for "Copy summary". */
    copy: () => string;
    files: ExportFile[];
    disabled?: boolean;
    label?: string;
  } = $props();

  let note = $state<{ text: string; error: boolean } | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  function say(text: string, error = false) {
    note = { text, error };
    clearTimeout(timer);
    timer = setTimeout(() => (note = null), 4000);
  }
  $effect(() => () => clearTimeout(timer));

  async function copySummary() {
    try { await copyGeneratedText(async () => copy()); say("Summary copied"); }
    catch { say("Couldn't copy to the clipboard", true); }
  }
  async function save(f: ExportFile) {
    try {
      const r = await saveReport(f.filename, f.build());
      say(r.saved ? `Saved ${f.filename}` : "Save cancelled");
    } catch (e) { say(e instanceof Error ? e.message : "Couldn't save the report", true); }
  }
</script>

<div class="export" role="group" aria-label={label}>
  <Button size="sm" variant="ghost" icon="copy" {disabled} onclick={copySummary}>Copy summary</Button>
  {#each files as f (f.filename)}
    <Button size="sm" variant="ghost" icon="download" {disabled} onclick={() => save(f)}>{f.label}</Button>
  {/each}
  <span class="note" class:err={note?.error} role="status" aria-live="polite">{note?.text ?? ""}</span>
</div>

<style>
  .export { position: relative; display: inline-flex; align-items: center; gap: 2px; flex-wrap: wrap; }
  /* Floats under the buttons so announcing a result never pushes the page around. */
  .note { position: absolute; top: calc(100% + 4px); right: 0; z-index: 5; white-space: nowrap; padding: 4px 10px; border-radius: var(--r-md); background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-md); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); pointer-events: none; }
  .note.err { color: var(--danger); border-color: var(--danger); }
  .note:empty { display: none; }
</style>
