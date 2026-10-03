<script lang="ts">
  import { untrack } from "svelte";
  // Pin a port with an optional label — works for free ports too (you're told when they start).
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import TextField from "./ui/TextField.svelte";
  import NumberInput from "./ui/NumberInput.svelte";
  import Callout from "./ui/Callout.svelte";
  import { validateLabel, validatePort } from "../lib/validate";

  let { port: initialPort = null, label: initialLabel = "", pinned = false, inUse, onsave, onunpin, onclose }: {
    port?: number | null; label?: string; pinned?: boolean;
    /** Describes what is on a port right now (null when free). */
    inUse: (port: number) => string | null;
    onsave: (port: number, label: string) => Promise<void>;
    onunpin: (port: number) => Promise<void>;
    onclose: () => void;
  } = $props();

  let port = $state<number | null>(untrack(() => initialPort));
  let label = $state(untrack(() => initialLabel));
  let submitted = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  const portError = $derived(submitted ? validatePort(port) : null);
  const labelError = $derived(validateLabel(label));
  const holder = $derived(port && !validatePort(port) ? inUse(port) : null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    submitted = true;
    if (validatePort(port) || labelError) return;
    saving = true; error = null;
    try { await onsave(port!, label.trim()); onclose(); } catch (err) { error = String(err); } finally { saving = false; }
  }
</script>

<Dialog title={pinned ? `Edit pin :${initialPort}` : "Pin a port"} description="Pinned ports stay at the top of the list, and you're notified when they start or stop." icon="star" iconTone="warn" size="sm" {onclose} initialFocus={initialPort ? "#pin-label" : "#pin-port"}>
  <form id="pin-form" class="form" onsubmit={submit} novalidate>
    <NumberInput id="pin-port" label="Port" required min={1} max={65535} width="140px" bind:value={port} error={portError} hint={holder ? `In use by ${holder}` : port ? "Free right now — you'll be told when something starts on it." : "1–65535"} />
    <TextField id="pin-label" label="Label" optional placeholder="e.g. shop web, staging API" bind:value={label} maxlength={40} counter error={labelError} hint="Shown next to the port in the list and in notifications." clearable />
    {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  </form>
  {#snippet footer()}
    {#if pinned}<Button variant="danger-outline" onclick={async () => { await onunpin(initialPort!); onclose(); }}>Unpin</Button><span class="sp"></span>{/if}
    <Button variant="ghost" kbd="Esc" onclick={onclose}>Cancel</Button>
    <Button variant="primary" type="submit" form="pin-form" icon="star" loading={saving} loadingText="Saving…">{pinned ? "Save" : "Pin port"}</Button>
  {/snippet}
</Dialog>

<style>
  .form { display: grid; gap: var(--sp-5); padding-top: var(--sp-2); }
  .sp { flex: 1; }
</style>
