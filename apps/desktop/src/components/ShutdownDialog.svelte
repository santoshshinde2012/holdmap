<script lang="ts">
  import { untrack } from "svelte";
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import Checkbox from "./ui/Checkbox.svelte";
  import Callout from "./ui/Callout.svelte";
  import { shutdownPreviewLifetimeMs, type ShutdownPreview } from "../lib/power";

  let { preview, loading, error, busy, requested, onconfirm, onclose, onretry, demo = false }: {
    preview: ShutdownPreview | null;
    loading: boolean;
    error: string | null;
    busy: boolean;
    requested: boolean;
    onconfirm: () => void;
    onclose: () => void;
    onretry?: () => void;
    demo?: boolean;
  } = $props();

  let understood = $state(false);
  let submitted = $state(false);
  let expired = $state(true);
  const previewId = $derived(preview?.confirmation_id ?? null);
  $effect(() => {
    // A re-render of the same token must not renew its lifetime or acknowledgement.
    const id = previewId;
    const lifetime = id ? untrack(() => shutdownPreviewLifetimeMs(preview)) : 0;
    understood = false;
    submitted = false;
    expired = lifetime === 0;
    if (!lifetime) return;
    const timer = setTimeout(() => (expired = true), lifetime);
    return () => clearTimeout(timer);
  });
  const ready = $derived(!!preview && !expired && understood && !loading && !busy && !requested && !submitted && !error);
  function confirm() {
    if (!ready) return;
    submitted = true;
    onconfirm();
  }
</script>

<Dialog role="alertdialog" title={demo ? "Simulate computer shutdown?" : "Shut down this computer?"} description="This affects the local computer, even when you're viewing a remote host." icon="monitor" iconTone="danger" size="md" dismissable={!busy} {onclose} initialFocus="[data-cancel]">
  <div class="content">
    {#if demo}
      <Callout tone="info" title="Browser demo">This is a simulation. No operating-system shutdown will be requested.</Callout>
    {/if}
    {#if requested}
      <div role="status"><Callout tone="ok" title={demo ? "Simulation complete" : "Shutdown requested"}>{demo ? "Your computer is still running. Nothing was stopped." : "Follow any operating-system prompts. Closing this dialog does not cancel the shutdown request. If the OS cancels afterward, quit Holdmap completely and reopen it before trying again."}</Callout></div>
    {:else}
      <Callout tone="warn" title="All apps, agents and local services will stop">Save your work and finish any important tasks before continuing. Unsaved work can be lost.</Callout>
      {#if loading}
        <p class="note" role="status">Checking this computer…</p>
      {:else if preview}
        <dl class="computer"><div><dt>Computer</dt><dd>{preview.hostname || "This computer"}</dd></div><div><dt>Operating system</dt><dd>{preview.platform}</dd></div></dl>
        {#if expired}
          <Callout tone="warn" title="Confirmation expired">Review a new preview before continuing. Nothing shuts down automatically.</Callout>
        {:else}
          <p class="note">Your operating system may ask for permission or cancel the request. This confirmation expires; it never schedules an automatic shutdown.</p>
        {/if}
        <Checkbox tone="danger" bind:checked={understood} disabled={expired || loading || busy || submitted || !!error} label="I saved my work and understand this shuts down all apps, agents and local services on this computer." />
      {/if}
      {#if error}<Callout tone="danger" title="Could not confirm the shutdown request">{error}</Callout>{/if}
    {/if}
  </div>
  {#snippet footer()}
    <Button data-cancel variant="secondary" disabled={busy} onclick={onclose}>{requested ? "Done" : "Cancel"}</Button>
    {#if !requested}
      {#if onretry && !loading && !busy && (expired || error || submitted)}
        <Button variant="secondary" onclick={onretry}>Review new preview</Button>
      {/if}
      <Button variant="danger" disabled={!ready} loading={busy} loadingText={demo ? "Simulating…" : "Requesting shutdown…"} onclick={confirm}>{demo ? "Simulate shutdown" : "Shut down this computer"}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .content { display: flex; flex-direction: column; gap: var(--sp-4); }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-body); line-height: var(--lh-body); }
  .computer { margin: 0; padding: var(--sp-3); display: grid; gap: var(--sp-2); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); font-size: var(--fs-body); line-height: var(--lh-body); }
  .computer div { display: grid; grid-template-columns: 132px minmax(0, 1fr); gap: var(--sp-3); }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); overflow-wrap: anywhere; }
  @media (max-width: 400px) { .computer div { grid-template-columns: 1fr; gap: var(--sp-1); } }
</style>
