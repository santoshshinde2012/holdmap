<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import ActionDock from "./ui/ActionDock.svelte";
  import Callout from "./ui/Callout.svelte";
  import { humanBytes } from "../lib/format";
  import { initialRecordingSupport, resolveRecordingSupport, type RecordingSession } from "../lib/screen-record-backend";
  import { captureScreenSnapshot } from "../lib/screen-snapshot-backend";
  import { snapshotProvider, type SnapshotSource } from "../lib/screen-snapshot";
  import { createSnapshotController, initialSnapshotState } from "../lib/snapshot-controller";
  import { saveSnapshot } from "../lib/screen-record-save";

  let { session = null, blocked = false, onstatechange }: {
    session?: RecordingSession | null;
    blocked?: boolean;
    onstatechange?: (state: { open: boolean; busy: boolean; hasResult: boolean }) => void;
  } = $props();
  let capture = $state.raw(initialSnapshotState());
  let support = $state(initialRecordingSupport());
  let choice = $state<SnapshotSource>("app");
  let url = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveMessage = $state<string | null>(null);
  let previewError = $state(false);
  let disposed = false;
  let supportSequence = 0;
  const provider = $derived(snapshotProvider(session));
  const result = $derived(capture.result);
  $effect(() => {
    // A saved choice must still name a source offered in the current video context.
    if ((choice === "screen" && provider) || (choice === "recording" && !provider)) choice = "app";
  });
  const controller = createSnapshotController({
    async app(signal) {
      const root = document.getElementById("holdmap-capture-root");
      if (!root) throw new Error("The Holdmap view is not available. Reopen the window and try again.");
      const { captureAppSnapshot } = await import("../lib/app-snapshot");
      return captureAppSnapshot(root, signal);
    },
    screen(signal) {
      if (!support.backend) return Promise.reject(new Error(support.reason || "Screen capture is unavailable in this window."));
      return captureScreenSnapshot(support.backend, signal);
    },
    recording: () => provider,
    prepare: settleView,
  }, (next) => (capture = next));

  async function settleView(signal: AbortSignal) {
    await tick();
    if (signal.aborted) throw new DOMException("Snapshot cancelled.", "AbortError");
    const palette = document.querySelector('[role="dialog"][aria-label="Command palette"]');
    if (palette) await new Promise<void>((resolve, reject) => {
      // A command runs before the palette's outro finishes. Observe its actual removal
      // so a snapshot of this window cannot include the fading command controls.
      const clean = () => { clearTimeout(timer); observer.disconnect(); signal.removeEventListener("abort", abort); };
      const ready = () => { if (!palette.isConnected) { clean(); resolve(); } };
      const abort = () => { clean(); reject(new DOMException("Snapshot cancelled.", "AbortError")); };
      const observer = new MutationObserver(ready);
      const timer = setTimeout(() => { clean(); reject(new Error("Close the command palette, then try taking the snapshot again.")); }, 2_000);
      observer.observe(document.body, { childList: true, subtree: true });
      signal.addEventListener("abort", abort, { once: true });
      ready();
    });
    if (signal.aborted) throw new DOMException("Snapshot cancelled.", "AbortError");
    await new Promise<void>((resolve, reject) => {
      let frame = 0;
      const clean = () => { clearTimeout(timer); cancelAnimationFrame(frame); signal.removeEventListener("abort", abort); };
      const done = () => { clean(); resolve(); };
      const abort = () => { clean(); reject(new DOMException("Snapshot cancelled.", "AbortError")); };
      // Two frames remove the chooser from the selected surface. The deadline also works
      // when a minimized window throttles requestAnimationFrame.
      const timer = setTimeout(done, 250);
      signal.addEventListener("abort", abort, { once: true });
      frame = requestAnimationFrame(() => { frame = requestAnimationFrame(done); });
    });
  }

  function checkSupport() {
    const sequence = ++supportSequence;
    support = initialRecordingSupport();
    if (support.pending) void resolveRecordingSupport().then((next) => {
      if (!disposed && sequence === supportSequence) support = next;
    });
  }
  export function show() { checkSupport(); controller.show(); }
  export function cancel() { controller.cancel(); }
  export function captureRecording() { return controller.capture("recording"); }
  $effect(() => {
    const status = { open: capture.open, busy: capture.busy, hasResult: !!result };
    untrack(() => onstatechange?.(status));
  });
  $effect(() => { if (blocked && !saving && (capture.open || capture.busy)) untrack(() => controller.close()); });
  $effect(() => {
    const value = result;
    const next = value ? URL.createObjectURL(value.png.blob) : null;
    url = next; previewError = false; saveError = null; saveMessage = null;
    return () => { if (next) URL.revokeObjectURL(next); };
  });
  onDestroy(() => { disposed = true; supportSequence++; controller.dispose(); });

  function choose() { choice = "app"; saveError = null; saveMessage = null; controller.choose(); }
  async function save() {
    if (!result || saving) return;
    saving = true; saveError = null; saveMessage = null;
    try {
      const outcome = await saveSnapshot(result.png);
      if (outcome.saved) saveMessage = outcome.path ? "Saved to " + outcome.path : "Download started. Check your browser's downloads for the PNG.";
    } catch (cause) {
      saveError = cause instanceof Error ? cause.message : String(cause);
    } finally { saving = false; }
  }
</script>

{#if capture.open}
  <div data-snapshot-ui>
    <Dialog title={capture.phase === "ready" ? "Snapshot preview" : "Take snapshot"}
      description={capture.phase === "ready" ? "Save this PNG on your computer. Nothing is uploaded." : "Capture the holdmap view or window. Video recording can continue while you take a snapshot."}
      icon="camera" size="lg" animate={false} dismissable={!saving} onclose={() => controller.close()} initialFocus={capture.phase === "choose" ? "[data-primary]" : "self"}>
      {#if capture.phase === "choose"}
        <ActionDock label="Snapshot source" value={choice} options={[
          { value: "app", label: "Holdmap view", icon: "panel" },
          provider
            ? { value: "recording", label: "Recording window", icon: "video" }
            : { value: "screen", label: "Holdmap window", icon: "monitor", disabled: support.pending || !support.backend },
        ]} onchange={(value) => (choice = value)} />
        <div class="source-note">
          {#if choice === "app"}
            <h3>This Holdmap view</h3>
            <p>Capture the visible app, including its current map and details. The snapshot controls are excluded. No screen-sharing permission is needed.</p>
          {:else if choice === "recording"}
            <h3>The window already being recorded</h3>
            <p>Capture the holdmap window being recorded without stopping the video.</p>
          {:else}
            <h3>The holdmap window</h3>
            <p>Captures only the holdmap window, as the system sees it. Anything else is refused. Sharing ends as soon as the PNG is captured; no audio or video is recorded.</p>
          {/if}
        </div>
        {#if !provider && support.pending}<p class="note" role="status">Checking screen capture support…</p>
        {:else if !provider && !support.backend}<Callout tone="info" title="Holdmap snapshots are available"><p>{support.reason || "Screen sharing is unavailable here."} You can still capture the Holdmap view.</p></Callout>{/if}
        {#if capture.error}<Callout tone="danger" title="Couldn't take snapshot"><p>{capture.error}</p>{#if result}<p>Your previous snapshot is still available.</p>{/if}</Callout>{/if}
      {:else if result && url}
        <div class="preview"><img src={url} alt="PNG snapshot preview" onerror={() => (previewError = true)} /></div>
        {#if previewError}<p class="note">This window couldn't display the preview. The PNG is still available to save.</p>{/if}
        <p class="meta">PNG · {result.width}×{result.height} · {humanBytes(result.png.bytes)} · {result.source === "app" ? "Holdmap view" : result.source === "recording" ? "Recording window" : "Holdmap window"}</p>
        <p class="note">Close keeps the latest snapshot for this session. Save it to keep it after reloading or quitting. A new snapshot replaces it only after capture succeeds.</p>
        {#if session}<p class="note">Your video is still recording.</p>{/if}
        {#if saveError}<Callout tone="danger" title="Couldn't save snapshot"><p>{saveError}</p><p>Your PNG is still available. Try saving again.</p></Callout>{/if}
        {#if saveMessage}<p class="note" role="status">{saveMessage}</p>{/if}
      {/if}
      {#snippet footer()}
        {#if capture.phase === "choose"}
          {#if result}<Button variant="ghost" onclick={() => controller.review()}>Review last snapshot</Button>{/if}
          <Button onclick={() => controller.close()}>Cancel</Button>
          <Button variant="primary" icon="camera" disabled={choice === "recording" ? !provider : choice === "screen" ? !!provider || !support.backend || support.pending : false} onclick={() => void controller.capture(choice)} data-primary>{choice === "screen" ? "Capture window" : "Capture PNG"}</Button>
        {:else}
          <Button variant="ghost" disabled={saving} onclick={() => controller.discard()}>Discard snapshot</Button>
          <Button disabled={saving} onclick={choose}>New snapshot</Button>
          <Button disabled={saving} onclick={() => controller.close()}>Close</Button>
          <Button variant="primary" icon="download" loading={saving} loadingText="Saving PNG…" onclick={() => void save()} data-primary>Download PNG</Button>
        {/if}
      {/snippet}
    </Dialog>
  </div>
{/if}

<style>
  .source-note { padding: var(--sp-4) 0; }
  h3 { margin: 0 0 var(--sp-2); color: var(--text); font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  p { margin: 0; }
  .source-note p, .note { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body); overflow-wrap: anywhere; }
  .note { margin: var(--sp-3) 0; }
  .meta { color: var(--text-2); font-size: var(--fs-body-sm); font-variant-numeric: tabular-nums; }
  .preview { margin-bottom: var(--sp-3); border: 1px solid var(--border); border-radius: var(--r-md); overflow: hidden; background: var(--surface-2); }
  .preview img { display: block; width: 100%; max-height: 360px; object-fit: contain; }
</style>
