<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Callout from "./ui/Callout.svelte";
  import { fileManagerLabel, type FolderTarget } from "../lib/folders";

  let { target, ownerName, available, platform, demo = false, onreveal, oneditor, onclose }: {
    target: FolderTarget;
    ownerName: string;
    available: boolean;
    platform?: string;
    demo?: boolean;
    onreveal: (agentId: string, path: string) => void | Promise<void | boolean>;
    oneditor: (agentId: string, path: string) => void | Promise<void | boolean>;
    onclose: () => void;
  } = $props();

  let busy = $state<"reveal" | "editor" | "copy" | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let surface = $state<HTMLDivElement | undefined>();
  // Disabling the focused opener must not send keyboard focus behind the modal.
  $effect.pre(() => {
    if (!available && surface?.contains(document.activeElement) && document.activeElement?.matches("[data-folder-open]")) surface.focus({ preventScroll: true });
  });

  function pending(kind: "reveal" | "editor" | "copy") {
    surface?.focus({ preventScroll: true });
    busy = kind;
  }

  async function open(kind: "reveal" | "editor") {
    if (busy || !available) return;
    pending(kind);
    error = null;
    try {
      const result = await (kind === "reveal" ? onreveal : oneditor)(target.agentId, target.path);
      if (result !== false) onclose();
      else error = "Could not open this folder. Check the application message for details and try again.";
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }

  async function copy() {
    if (busy) return;
    pending("copy");
    error = null;
    copied = false;
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard access is unavailable. Select and copy the path above.");
      await navigator.clipboard.writeText(target.path);
      copied = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<Dialog title="Folder actions" description="Choose how to open this folder on the local computer." icon="folder" size="sm" closeButton={false} dismissable={!busy} {onclose} initialFocus="[data-folder-close]" bind:surface>
  <div class="content">
    <p class="owner">Listed for {ownerName}</p>
    <code class="path selectable">{target.path}</code>
    {#if demo}<Callout tone="info" size="sm">Browser demo: opening folders and editors is simulated. Copy path copies the displayed sample path.</Callout>{/if}
    {#if !available}<Callout tone="warn" size="sm">This folder is no longer in the current scan for {ownerName}. You can still copy its path.</Callout>{/if}
    <div class="actions">
      <Button data-folder-open full icon="folder" disabled={!available || !!busy} loading={busy === "reveal"} onclick={() => open("reveal")}>{fileManagerLabel(platform)}</Button>
      <Button data-folder-open full icon="code" disabled={!available || !!busy} loading={busy === "editor"} onclick={() => open("editor")}>Open in editor</Button>
      <Button full icon={copied ? "check" : "copy"} disabled={!!busy} loading={busy === "copy"} onclick={copy}>Copy path</Button>
    </div>
    {#if copied}<p class="feedback" role="status">Path copied</p>{/if}
    {#if error}<Callout tone="danger" size="sm" title="Folder action unavailable">{error}</Callout>{/if}
  </div>
  {#snippet footer()}<Button data-folder-close disabled={!!busy} onclick={onclose}>Close</Button>{/snippet}
</Dialog>

<style>
  .content { display: flex; flex-direction: column; gap: var(--sp-3); }
  .owner { margin: 0; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .path { display: block; white-space: pre-wrap; overflow-wrap: anywhere; padding: var(--sp-3); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); color: var(--text); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .actions { display: grid; gap: var(--sp-2); }
  .feedback { margin: 0; color: var(--ok); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
</style>
