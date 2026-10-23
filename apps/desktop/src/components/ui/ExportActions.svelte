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
  .export { display: inline-flex; align-items: center; gap: 2px; flex-wrap: wrap; }
  .note { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); margin-left: 4px; }
  .note.err { color: var(--danger); }
  .note:empty { display: none; }
</style>
