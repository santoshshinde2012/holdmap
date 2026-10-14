<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import { humanBytes } from "../lib/format";
  import { captureSupport, formatClock, MAX_DURATION_MS, type CaptureFile, type CaptureResult } from "../lib/screen-record";
  import { saveRecording } from "../lib/screen-record-save";

  let {
    phase,
    error = null,
    errorTitle = "Couldn't start",
    result = null,
    busy = false,
    starting = false,
    supported = captureSupport().display,
    onstart,
    onclose,
  }: {
    phase: "confirm" | "encoding" | "ready";
    error?: string | null;
    errorTitle?: string;
    result?: CaptureResult | null;
    busy?: boolean;
    starting?: boolean;
    supported?: boolean;
    onstart: () => void;
    onclose: () => void;
  } = $props();

  let mp4Url = $state<string | null>(null);
  let gifUrl = $state<string | null>(null);
  let saving = $state<"mp4" | "gif" | null>(null);
  let saveError = $state<string | null>(null);
  let saveMessage = $state<string | null>(null);
  let previewError = $state(false);
  $effect(() => {
    const nextMp4 = result?.mp4 ? URL.createObjectURL(result.mp4.blob) : null;
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
      ? "Encoding MP4 and GIF from the capture. This stays on this computer."
      : supported ? "Choose a screen or window. Microphone and system audio are not captured." : "Screen recording needs screen sharing support in this window.");

  async function download(file: CaptureFile, format: "mp4" | "gif") {
    if (saving) return;
    saving = format;
    saveError = null;
    saveMessage = null;
    try {
      const outcome = await saveRecording(file);
      if (outcome.saved) saveMessage = outcome.path ? `Saved to ${outcome.path}` : "Download started. Check your browser's downloads for the file.";
    } catch (cause) {
      saveError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = null;
    }
  }
</script>

<Dialog {title} {description} icon="video" size="lg" dismissable={!saving} closeButton={phase !== "encoding"} {onclose} initialFocus={phase === "confirm" && supported ? "[data-primary]" : "self"}>
  {#if phase === "confirm"}
    {#if supported}
      <Callout tone="info" title="Local only">
        <p>Choose which display to capture in the screen picker. The recording never leaves this machine. Maximum {Math.round(MAX_DURATION_MS / 60_000)} minutes. <strong>MP4</strong> and <strong>GIF</strong> exports depend on this window's encoding support.</p>
      </Callout>
    {:else}
      <Callout tone="warn" title="Screen capture unavailable">
        <p>This window does not offer screen sharing. Try a browser that supports it, or use your operating system's screen recorder.</p>
      </Callout>
    {/if}
    {#if busy}<p class="waiting" role="status">{starting ? "Preparing recording. Cancel stops sharing and discards this request." : "Choose a screen in the picker. Cancel here to ignore this request; you may still need to dismiss the system picker."}</p>{/if}
    {#if error}<div class="gap"><Callout tone="danger" title={errorTitle}>{error}</Callout></div>{/if}
  {:else if phase === "encoding"}
    <p class="status" role="status" aria-busy="true">Encoding MP4 and GIF…</p>
    <p class="waiting">Screen sharing has stopped. Discard removes this recording without saving files.</p>
  {:else if result}
    <div class="preview">
      {#if mp4Url}
        <video src={mp4Url} controls playsinline muted aria-label="MP4 preview" onerror={() => { previewError = true; }}></video>
      {:else if gifUrl}
        <img src={gifUrl} alt="GIF preview" />
      {/if}
    </div>
    {#if previewError}<p class="waiting">This window cannot play the MP4 preview. You can still save the file and open it in a video player.</p>{/if}
    <p class="meta">{formatClock(result.durationMs)} · {result.width}×{result.height}{#if result.mp4} · MP4 {humanBytes(result.mp4.bytes)}{/if}{#if result.gif} · GIF {humanBytes(result.gif.bytes)}{/if}</p>
    {#if result.warnings.length}
      <Callout tone="warn" title="Partial export">
        {#each result.warnings as warning}<p>{warning}</p>{/each}
      </Callout>
    {/if}
    {#if saveError}<div class="gap"><Callout tone="danger" title="Couldn't save recording"><p>{saveError}</p><p>Your recording is still available. Try saving it again.</p></Callout></div>{/if}
    {#if saveMessage}<p class="waiting" role="status">{saveMessage}</p>{/if}
  {/if}

  {#snippet footer()}
    {#if phase === "confirm"}
      <Button onclick={onclose}>Cancel</Button>
      {#if supported}<Button variant="primary" icon="video" loading={busy} loadingText={starting ? "Starting recording…" : "Waiting for permission…"} onclick={onstart} data-primary>Choose screen…</Button>{/if}
    {:else if phase === "encoding"}
      <Button onclick={onclose}>Discard recording</Button>
    {:else}
      <Button disabled={!!saving} onclick={onclose}>Close</Button>
      {#if result?.gif && result?.mp4}<Button icon="download" disabled={!!saving} loading={saving === "gif"} loadingText="Saving GIF…" onclick={() => result?.gif && void download(result.gif, "gif")}>Download GIF</Button>{/if}
      {#if result?.mp4}<Button variant="primary" icon="download" disabled={!!saving} loading={saving === "mp4"} loadingText="Saving MP4…" onclick={() => result?.mp4 && void download(result.mp4, "mp4")} data-primary>Download MP4</Button>
      {:else if result?.gif}<Button variant="primary" icon="download" disabled={!!saving} loading={saving === "gif"} loadingText="Saving GIF…" onclick={() => result?.gif && void download(result.gif, "gif")} data-primary>Download GIF</Button>{/if}
    {/if}
  {/snippet}
</Dialog>

<style>
  .gap { margin-top: var(--sp-3); }
  .status { margin: 0; color: var(--text-2); }
  .waiting { margin: var(--sp-3) 0 0; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body); overflow-wrap: anywhere; }
  .preview { margin: 0 0 var(--sp-3); border-radius: var(--r-md); overflow: hidden; background: var(--surface-2); border: 1px solid var(--border); }
  .preview video, .preview img { display: block; width: 100%; max-height: 360px; object-fit: contain; background: #0b0d12; }
  .meta { margin: 0 0 var(--sp-3); color: var(--muted); font-size: var(--fs-body-sm); font-variant-numeric: tabular-nums; }
</style>
