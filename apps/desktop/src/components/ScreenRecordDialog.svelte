<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import { humanBytes } from "../lib/format";
  import { downloadBlob, formatClock, MAX_DURATION_MS, type CaptureResult } from "../lib/screen-record";

  let {
    phase,
    error = null,
    result = null,
    busy = false,
    onstart,
    onclose,
  }: {
    phase: "confirm" | "encoding" | "ready";
    error?: string | null;
    result?: CaptureResult | null;
    busy?: boolean;
    onstart: () => void;
    onclose: () => void;
  } = $props();

  let mp4Url = $state<string | null>(null);
  let gifUrl = $state<string | null>(null);
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
      : "Choose a screen or window. Microphone and system audio are not captured.");
</script>

<Dialog {title} {description} icon="video" size="lg" dismissable={phase !== "encoding"} closeButton={phase !== "encoding"} {onclose} initialFocus={phase === "confirm" ? "[data-primary]" : "self"}>
  {#if phase === "confirm"}
    <Callout tone="info" title="Local only">
      <p>Holdmap asks the operating system which display to capture. The recording never leaves this machine. Maximum {Math.round(MAX_DURATION_MS / 60_000)} minutes. You can download <strong>MP4</strong> and <strong>GIF</strong> when it stops.</p>
    </Callout>
    {#if error}<div class="gap"><Callout tone="danger" title="Couldn't start">{error}</Callout></div>{/if}
  {:else if phase === "encoding"}
    <p class="status" role="status" aria-busy="true">Encoding MP4 and GIF…</p>
  {:else if result}
    <div class="preview">
      {#if mp4Url}
        <video src={mp4Url} controls playsinline muted aria-label="MP4 preview"></video>
      {:else if gifUrl}
        <img src={gifUrl} alt="GIF preview" />
      {/if}
    </div>
    <p class="meta">{formatClock(result.durationMs)} · {result.width}×{result.height}{#if result.mp4} · MP4 {humanBytes(result.mp4.bytes)}{/if}{#if result.gif} · GIF {humanBytes(result.gif.bytes)}{/if}</p>
    {#if result.warnings.length}
      <Callout tone="warn" title="Partial export">
        {#each result.warnings as warning}<p>{warning}</p>{/each}
      </Callout>
    {/if}
  {/if}

  {#snippet footer()}
    {#if phase === "confirm"}
      <Button onclick={onclose}>Cancel</Button>
      <Button variant="primary" icon="video" loading={busy} loadingText="Waiting for permission…" onclick={onstart} data-primary>Choose screen…</Button>
    {:else if phase === "encoding"}
      <Button disabled>Preparing…</Button>
    {:else}
      <Button onclick={onclose}>Close</Button>
      {#if result?.gif && result?.mp4}<Button icon="download" onclick={() => result?.gif && downloadBlob(result.gif.blob, result.gif.filename)}>Download GIF</Button>{/if}
      {#if result?.mp4}<Button variant="primary" icon="download" onclick={() => result?.mp4 && downloadBlob(result.mp4.blob, result.mp4.filename)} data-primary>Download MP4</Button>
      {:else if result?.gif}<Button variant="primary" icon="download" onclick={() => result?.gif && downloadBlob(result.gif.blob, result.gif.filename)} data-primary>Download GIF</Button>{/if}
    {/if}
  {/snippet}
</Dialog>

<style>
  .gap { margin-top: var(--sp-3); }
  .status { margin: 0; color: var(--text-2); }
  .preview { margin: 0 0 var(--sp-3); border-radius: var(--r-md); overflow: hidden; background: var(--surface-2); border: 1px solid var(--border); }
  .preview video, .preview img { display: block; width: 100%; max-height: 360px; object-fit: contain; background: #0b0d12; }
  .meta { margin: 0 0 var(--sp-3); color: var(--muted); font-size: var(--fs-body-sm); font-variant-numeric: tabular-nums; }
</style>
