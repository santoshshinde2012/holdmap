<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import CopyValue from "./ui/CopyValue.svelte";
  import SegmentedControl from "./ui/SegmentedControl.svelte";
  import Select from "./ui/Select.svelte";
  import { humanBytes } from "../lib/format";
  import { captureSupport, formatClock, type CaptureFile, type CaptureResult, type RecordingLimit } from "../lib/screen-record";
  import {
    DEFAULT_PLAN, DURATION_CHOICES_MS, PRESET_SUMMARY, estimateLines, formatChoice, formatLength, withDuration, withFormat, withPreset,
    type FormatChoice, type RecordingPlan, type RecordingPresetId,
  } from "../lib/screen-record-plan";
  import { revealSavedRecording, saveRecording } from "../lib/screen-record-save";

  let {
    phase,
    error = null,
    errorTitle = "Couldn't start",
    result = null,
    busy = false,
    starting = false,
    supported = captureSupport().display,
    checkingSupport = false,
    unsupportedReason = null,
    plan = DEFAULT_PLAN,
    backend = null,
    stoppedBy = null,
    onplan = () => {},
    onstart,
    onclose,
    ondiscard = onclose,
  }: {
    phase: "confirm" | "encoding" | "ready";
    error?: string | null;
    errorTitle?: string;
    result?: CaptureResult | null;
    busy?: boolean;
    starting?: boolean;
    supported?: boolean;
    checkingSupport?: boolean;
    unsupportedReason?: string | null;
    plan?: RecordingPlan;
    backend?: "browser" | "native" | null;
    stoppedBy?: RecordingLimit | null;
    onplan?: (plan: RecordingPlan) => void;
    onstart: () => void;
    onclose: () => void;
    ondiscard?: () => void;
  } = $props();

  const desktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  const presets: { value: RecordingPresetId; label: string }[] = [
    { value: "clip", label: "Share a clip" },
    { value: "session", label: "Full session" },
    { value: "custom", label: "Custom" },
  ];
  const formats: { value: FormatChoice; label: string }[] = [
    { value: "mp4", label: "MP4" },
    { value: "gif", label: "GIF" },
    { value: "both", label: "Both" },
  ];
  const lengths = DURATION_CHOICES_MS.map((ms) => ({ value: String(ms), label: formatLength(ms) }));
  const lines = $derived(estimateLines(plan, backend, humanBytes));
  const limitNote: Record<RecordingLimit, string> = {
    duration: "Recording stopped by itself at the length you chose.",
    size: "Recording stopped by itself because a file reached its size limit. Everything captured so far is saved.",
    frames: "Recording stopped by itself because the GIF reached its frame limit.",
  };

  let mp4Url = $state<string | null>(null);
  let gifUrl = $state<string | null>(null);
  let saving = $state<"mp4" | "gif" | null>(null);
  let saveError = $state<string | null>(null);
  let saved = $state<{ format: "mp4" | "gif"; path: string | null }[]>([]);
  let revealError = $state<string | null>(null);
  let previewError = $state(false);
  $effect(() => {
    const nextMp4 = result?.mp4 && !result.mp4.native ? URL.createObjectURL(result.mp4.blob) : null;
    const nextGif = result?.gif ? URL.createObjectURL(result.gif.blob) : null;
    mp4Url = nextMp4;
    gifUrl = nextGif;
    return () => {
      if (nextMp4) URL.revokeObjectURL(nextMp4);
      if (nextGif) URL.revokeObjectURL(nextGif);
    };
  });

  const title = $derived(phase === "ready" ? "Download recording" : phase === "encoding" ? "Preparing files" : "Record screen");
  const description = $derived(phase === "ready"
    ? "Files stay on this computer. Nothing is uploaded."
    : phase === "encoding"
      ? `Finishing ${plan.formats.map((format) => format.toUpperCase()).join(" and ")}. This stays on this computer.`
      : checkingSupport ? "Checking screen recording support in this window."
        : supported ? "Pick what to record, then choose a screen or window." : "Screen recording needs screen sharing support in this window.");

  function choosePreset(id: RecordingPresetId) { onplan(withPreset(id, plan)); }
  function chooseFormat(choice: FormatChoice) { onplan(withFormat(plan, choice)); }
  function chooseLength(value: string) { onplan(withDuration(plan, Number(value))); }

  async function download(file: CaptureFile, format: "mp4" | "gif") {
    if (saving) return;
    saving = format;
    saveError = null;
    revealError = null;
    try {
      const outcome = await saveRecording(file);
      if (outcome.saved) saved = [...saved.filter((entry) => entry.format !== format), { format, path: outcome.path ?? null }];
    } catch (cause) {
      saveError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = null;
    }
  }

  async function copyPath(text: string): Promise<boolean> {
    try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
  }

  async function reveal(path: string) {
    revealError = null;
    try { await revealSavedRecording(path); }
    catch (cause) { revealError = cause instanceof Error ? cause.message : "Couldn't open the folder."; }
  }
</script>

<Dialog {title} {description} icon="video" size="lg" dismissable={!saving} closeButton={phase !== "encoding"} {onclose} initialFocus={phase === "confirm" && supported ? "[data-primary]" : "self"}>
  {#if phase === "confirm"}
    {#if checkingSupport}
      <p class="status" role="status" aria-busy="true">Checking screen recording support…</p>
    {:else if supported}
      <div class="plan">
        <SegmentedControl label="Preset" size="md" full value={plan.preset} options={presets} onchange={choosePreset} />
        <p class="hint" data-preset-summary>{PRESET_SUMMARY[plan.preset]}</p>
        <div class="row">
          <SegmentedControl label="Format" size="md" value={formatChoice(plan)} options={formats} onchange={chooseFormat} />
          <Select label="Stop after" size="md" width="150px" value={String(plan.maxDurationMs)} options={lengths} onchange={chooseLength} />
        </div>
        <ul class="estimate" aria-label="Expected size and length" aria-live="polite">
          {#each lines as line}<li>{line}</li>{/each}
        </ul>
      </div>
      <Callout tone="info" title="Local only">
        <p>You choose the screen or window in the system picker, and Holdmap records only that. No microphone or system audio. Nothing is uploaded.</p>
        <p>Recording continues while Holdmap is hidden. Use the red timer in the title bar to pause or stop.</p>
      </Callout>
    {:else}
      <Callout tone="warn" title="Screen capture unavailable">
        <p>{unsupportedReason || "This window does not offer screen sharing. Try a browser that supports it, or use your operating system's screen recorder."}</p>
      </Callout>
    {/if}
    {#if busy}<p class="waiting" role="status">{starting ? "Preparing recording. Cancel stops sharing and discards this request." : "Choose a screen in the picker. Cancel here to ignore this request; you may still need to dismiss the system picker."}</p>{/if}
    {#if error}<div class="gap"><Callout tone="danger" title={errorTitle}>{error}</Callout></div>{/if}
  {:else if phase === "encoding"}
    <p class="status" role="status" aria-busy="true">Finishing your recording…</p>
    <p class="waiting">Long recordings take a little longer. Discard removes this recording without saving files.</p>
  {:else if result}
    <div class="preview">
      {#if mp4Url}
        <video src={mp4Url} controls playsinline muted aria-label="MP4 preview" onerror={() => { previewError = true; }}></video>
      {:else if gifUrl}
        <img src={gifUrl} alt="GIF preview" />
      {:else}
        <p class="nopreview">This {formatClock(result.durationMs)} recording stays on disk instead of loading here. Save it to watch it in your video player.</p>
      {/if}
    </div>
    {#if previewError}<p class="waiting">This window cannot play the MP4 preview. You can still save the file and open it in a video player.</p>{/if}
    <p class="meta">{formatClock(result.durationMs)} · {result.width}×{result.height}{#if result.mp4} · MP4 {humanBytes(result.mp4.bytes)}{/if}{#if result.gif} · GIF {humanBytes(result.gif.bytes)}{/if}</p>
    {#if stoppedBy}<div class="gap"><Callout tone="info" title="Stopped automatically">{limitNote[stoppedBy]}</Callout></div>{/if}
    {#if result.warnings.length}
      <div class="gap"><Callout tone="warn" title="Partial export">
        {#each result.warnings as warning}<p>{warning}</p>{/each}
      </Callout></div>
    {/if}
    {#if saveError}<div class="gap"><Callout tone="danger" title="Couldn't save recording"><p>{saveError}</p><p>Your recording is still available. Try saving it again.</p></Callout></div>{/if}
    {#each saved as entry (entry.format)}
      <div class="gap" role="status" data-saved={entry.format}>
        <Callout tone="ok" title={`${entry.format.toUpperCase()} saved`}>
          {#snippet actions()}{#if entry.path && desktop}<Button size="sm" icon="folder" onclick={() => void reveal(entry.path!)}>Show in folder</Button>{/if}{/snippet}
          {#if entry.path}
            <CopyValue value={entry.path} mono wrap what="Path" oncopy={copyPath} />
          {:else}
            <p>Download started. Check your browser's downloads for the file.</p>
          {/if}
        </Callout>
      </div>
    {/each}
    {#if revealError}<p class="waiting" role="alert">{revealError}</p>{/if}
    <p class="waiting retention">Close keeps this recording for this session. Save a file to keep it after reloading or quitting. Discard removes it and lets you record again.</p>
  {/if}

  {#snippet footer()}
    {#if phase === "confirm"}
      <Button onclick={onclose}>Cancel</Button>
      {#if supported}<Button variant="primary" icon="video" loading={busy} loadingText={starting ? "Starting recording…" : "Waiting for permission…"} onclick={onstart} data-primary>Choose screen…</Button>{/if}
    {:else if phase === "encoding"}
      <Button onclick={onclose}>Discard recording</Button>
    {:else}
      <Button variant="ghost" disabled={!!saving} onclick={ondiscard}>Discard recording</Button>
      <Button disabled={!!saving} onclick={onclose}>Close</Button>
      {#if result?.gif && result?.mp4}<Button icon="download" disabled={!!saving} loading={saving === "gif"} loadingText="Saving GIF…" onclick={() => result?.gif && void download(result.gif, "gif")}>Download GIF</Button>{/if}
      {#if result?.mp4}<Button variant="primary" icon="download" disabled={!!saving} loading={saving === "mp4"} loadingText="Saving MP4…" onclick={() => result?.mp4 && void download(result.mp4, "mp4")} data-primary>Download MP4</Button>
      {:else if result?.gif}<Button variant="primary" icon="download" disabled={!!saving} loading={saving === "gif"} loadingText="Saving GIF…" onclick={() => result?.gif && void download(result.gif, "gif")} data-primary>Download GIF</Button>{/if}
    {/if}
  {/snippet}
</Dialog>

<style>
  .gap { margin-top: var(--sp-3); }
  .plan { display: grid; gap: var(--sp-3); margin-bottom: var(--sp-3); }
  .row { display: flex; align-items: flex-end; justify-content: space-between; gap: var(--sp-3); flex-wrap: wrap; }
  .hint { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .estimate { margin: 0; padding: var(--sp-2) var(--sp-3); list-style: none; display: grid; gap: 2px; border-radius: var(--r-md); background: var(--surface-2); border: 1px solid var(--border); color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-variant-numeric: tabular-nums; }
  .status { margin: 0; color: var(--text-2); }
  .waiting { margin: var(--sp-3) 0 0; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body); overflow-wrap: anywhere; }
  .retention { margin-bottom: var(--sp-3); }
  .preview { margin: 0 0 var(--sp-3); border-radius: var(--r-md); overflow: hidden; background: var(--surface-2); border: 1px solid var(--border); }
  .preview video, .preview img { display: block; width: 100%; max-height: 360px; object-fit: contain; background: #0b0d12; }
  .nopreview { margin: 0; padding: var(--sp-5) var(--sp-4); color: var(--text-2); font-size: var(--fs-body-sm); text-align: center; }
  .meta { margin: 0 0 var(--sp-3); color: var(--muted); font-size: var(--fs-body-sm); font-variant-numeric: tabular-nums; }
</style>
