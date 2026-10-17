<script lang="ts">
  // First-run welcome: what holdmap shows, where to start, and the integrations that touch the
  // system. Switches show the current settings; Skip only remembers that the welcome was seen.
  import { Button, Dialog, SegmentedControl, Switch } from "./ui";
  import { defaultChoices, type CurrentSettings, type FirstRunChoices, type StartView } from "../lib/onboarding";

  let { current, canAutostart, tray, platform, shortcut, mod, onapply, onskip }: {
    current: CurrentSettings;
    canAutostart: boolean;
    tray: boolean;
    platform: string;
    shortcut: string | null;
    mod: string;
    onapply: (choices: FirstRunChoices) => void;
    onskip: () => void;
  } = $props();

  // The dialog edits a local draft; nothing changes until "Start".
  // svelte-ignore state_referenced_locally
  let choices = $state<FirstRunChoices>(defaultChoices(current));
  const starts: { value: StartView; label: string; title: string }[] = [
    { value: "home", label: "Overview", title: "What's running, what needs a look" },
    { value: "agents", label: "Agents", title: "Agents, tools and apps with their folders, ports and access" },
    { value: "list", label: "Ports", title: "Every listening port" },
  ];
</script>

<Dialog title="Welcome to holdmap" description="See which agents, tools and dev servers are running, what they touch, and stop the right thing safely." icon="sparkles" size="md" onclose={onskip} initialFocus="[data-primary]">
  <div class="body">
    <section aria-labelledby="fr-start">
      <h3 id="fr-start">Start on</h3>
      <SegmentedControl label="Start view" bind:value={choices.start} options={starts} />
    </section>
    <section aria-labelledby="fr-optin">
      <h3 id="fr-optin">Your choice</h3>
      <p class="note">Change these any time in Settings. Scans stay on this computer.</p>
      <Switch bind:checked={choices.notify} label="Notify me about new and conflicting ports" />
      {#if canAutostart}<Switch bind:checked={choices.autostart} label="Launch holdmap at login" />{/if}
      <Switch bind:checked={choices.singleKeys} label="Single-key shortcuts" description="G graph · ⇧A agents · ⌫ stop (always asks first)" />
    </section>
    <p class="tips"><kbd>{mod}</kbd><kbd>K</kbd> opens every command.{#if tray} Closing the window keeps holdmap in the {platform === "macos" ? "menu bar" : "system tray"}{#if shortcut}; <kbd>{shortcut}</kbd> brings it back{/if}.{/if}</p>
  </div>
  {#snippet footer()}
    <Button onclick={onskip}>Skip</Button>
    <Button variant="primary" onclick={() => onapply(choices)} data-primary>Start</Button>
  {/snippet}
</Dialog>

<style>
  .body { display: grid; gap: var(--sp-5); }
  section { display: grid; gap: var(--sp-2); }
  h3 { margin: 0; font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); font-weight: var(--fw-semibold); text-transform: uppercase; color: var(--muted); }
  .note, .tips { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  kbd { font-family: var(--mono); }
</style>
