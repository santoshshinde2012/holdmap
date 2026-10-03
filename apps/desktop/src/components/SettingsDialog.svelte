<script lang="ts">
  import { untrack } from "svelte";
  // Preferences. Changes apply immediately (desktop convention) with a "Saved" confirmation;
  // invalid input never reaches the backend.
  import Dialog from "./ui/Dialog.svelte";
  import Icon from "./Icon.svelte";
  import Button from "./ui/Button.svelte";
  import Switch from "./ui/Switch.svelte";
  import Checkbox from "./ui/Checkbox.svelte";
  import Select from "./ui/Select.svelte";
  import SegmentedControl from "./ui/SegmentedControl.svelte";
  import NumberInput from "./ui/NumberInput.svelte";
  import SettingRow from "./ui/SettingRow.svelte";
  import SettingsGroup from "./ui/SettingsGroup.svelte";
  import CopyValue from "./ui/CopyValue.svelte";
  import Callout from "./ui/Callout.svelte";
  import Kbd from "./ui/Kbd.svelte";
  import { nextIndex } from "../lib/roving";
  import type { Density } from "../lib/rows";
  import { SETTINGS_SECTIONS, THEME_ICON, cadenceText, type SettingsActions, type SettingsModel, type SettingsSection, type Theme } from "../lib/settings";

  let { model, actions, onclose, section = $bindable("general") }: { model: SettingsModel; actions: SettingsActions; onclose: () => void; section?: SettingsSection } = $props();

  let status = $state<{ kind: "saving" | "saved" | "error"; text: string } | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string | null>>({});
  let interval = $state<number | null>(untrack(() => model.config.scan_interval_secs ?? 4));
  let historyLimit = $state<number | null>(untrack(() => model.config.history_limit));
  let confirmClear = $state(false);
  let navEls: HTMLButtonElement[] = $state([]);
  let t: ReturnType<typeof setTimeout> | undefined;

  async function save(key: string, f: () => Promise<void> | void, ok = "Saved") {
    busy[key] = true;
    errors[key] = null;
    status = { kind: "saving", text: "Saving…" };
    try {
      await f();
      status = { kind: "saved", text: ok };
      clearTimeout(t);
      t = setTimeout(() => (status = null), 1800);
    } catch (e) {
      errors[key] = String(e);
      status = { kind: "error", text: "Couldn't save — see the message above" };
    } finally {
      busy[key] = false;
    }
  }
  const idx = $derived(SETTINGS_SECTIONS.findIndex((s) => s.id === section));
  function navKey(e: KeyboardEvent) {
    const n = nextIndex(idx, e.key, SETTINGS_SECTIONS.length, { orientation: "both" });
    if (n !== null) { section = SETTINGS_SECTIONS[n].id; navEls[n]?.focus(); e.preventDefault(); }
  }
  const themes: { value: Theme; label: string; icon: string }[] = [
    { value: "system", label: "System", icon: THEME_ICON.system },
    { value: "light", label: "Light", icon: "sun" },
    { value: "dark", label: "Dark", icon: "moon" },
  ];
  let theme = $state<Theme>(untrack(() => model.theme));
  const densities: { value: Density; label: string; icon: string }[] = [
    { value: "comfortable", label: "Comfortable", icon: "list" },
    { value: "compact", label: "Compact", icon: "minus" },
  ];
  let density = $state<Density>(untrack(() => model.density));
</script>

<Dialog title="Settings" description="Startup, appearance, notifications and scanning for this computer." icon="sliders" size="xl" {onclose} initialFocus="[role=tab][aria-selected=true]">
  <div class="layout">
    <div class="nav" role="tablist" aria-orientation="vertical" aria-label="Settings sections" tabindex="-1" onkeydown={navKey}>
      {#each SETTINGS_SECTIONS as s, i (s.id)}
        <button bind:this={navEls[i]} type="button" role="tab" id="set-tab-{s.id}" aria-controls="set-panel" aria-selected={section === s.id} tabindex={section === s.id ? 0 : -1} class:on={section === s.id} onclick={() => (section = s.id)}>
          <Icon name={s.icon} size={14} />{s.label}
        </button>
      {/each}
    </div>

    <div class="panel" role="tabpanel" id="set-panel" aria-labelledby="set-tab-{section}">
      {#if section === "general"}
        <SettingsGroup title="Startup">
          <SettingRow label="Launch at login" description="Start portwise in the menu bar when you log in. The window stays hidden until you need it.">
            {#snippet children({ labelId, descId })}
              <Switch labelledby={labelId} describedby={descId} checked={model.autostart} busy={busy.autostart} onchange={(v) => save("autostart", () => actions.setAutostart(v), v ? "portwise will start at login" : "Launch at login off")} />
            {/snippet}
          </SettingRow>
        </SettingsGroup>
        {#if errors.autostart}<div class="err"><Callout tone="danger" size="sm">{errors.autostart}</Callout></div>{/if}
        <SettingsGroup title="Global shortcut" description="Bring portwise to the front from any app.">
          <SettingRow label="Show portwise" description={model.shortcut ? `Currently ${model.shortcut}.` : "No global shortcut is active."}>
            {#snippet children()}
              <Select labelHidden label="Global shortcut" width="190px" value={model.config.hotkey ?? "alt-p"} options={model.hotkeys.map((h) => ({ value: h.id, label: h.label }))} onchange={(v) => save("hotkey", () => actions.setHotkey(v))} />
            {/snippet}
          </SettingRow>
        </SettingsGroup>
        {#if errors.hotkey}<div class="err"><Callout tone="danger" size="sm" title="Shortcut unavailable">{errors.hotkey}</Callout></div>{/if}
      {:else if section === "appearance"}
        <SettingsGroup title="Display">
          <SettingRow label="Appearance" description="System follows your OS light/dark setting.">
            {#snippet children()}
              <SegmentedControl label="Theme" size="md" bind:value={theme} options={themes} onchange={(v) => save("theme", () => actions.setTheme(v))} />
            {/snippet}
          </SettingRow>
          <SettingRow label="List density" description="Comfortable rows are 44 px tall; compact fits more ports on screen at 36 px.">
            {#snippet children()}
              <SegmentedControl label="List density" size="md" bind:value={density} options={densities} onchange={(v) => save("density", () => actions.setDensity(v))} />
            {/snippet}
          </SettingRow>
          <SettingRow label="Reduce motion" description="Animations follow your system's “Reduce motion” accessibility setting.">
            {#snippet children()}<span class="ro">System</span>{/snippet}
          </SettingRow>
        </SettingsGroup>
        <p class="tip"><Kbd keys={["⇧", "L"]} size="sm" /> cycles the theme from anywhere.</p>
      {:else if section === "notifications"}
        <SettingsGroup title="Desktop notifications" description="portwise watches in the background and tells you when something changes.">
          <SettingRow label="Notify me" description="New listeners, port conflicts, and pinned ports that stop.">
            {#snippet children({ labelId, descId })}
              <Switch labelledby={labelId} describedby={descId} checked={model.config.notify} busy={busy.notify} onchange={(v) => save("notify", () => actions.setNotify(v, model.config.notify_dev_only), v ? "Notifications on" : "Notifications off")} />
            {/snippet}
          </SettingRow>
          <div class="row-in">
            <Checkbox label="Only dev servers and pinned ports" description="Skip system services and desktop apps that open ports." checked={model.config.notify_dev_only} disabled={!model.config.notify || busy.notify} onchange={(v) => save("notify", () => actions.setNotify(model.config.notify, v))} />
          </div>
        </SettingsGroup>
        {#if errors.notify}<div class="err"><Callout tone="danger" size="sm">{errors.notify}</Callout></div>{/if}
      {:else if section === "scanning"}
        <SettingsGroup title="Scanning">
          <SettingRow label="Scan every" description={interval ? cadenceText(interval) : "1–60 seconds."}>
            {#snippet children()}
              <NumberInput label="Scan interval" labelHidden min={1} max={60} unit="s" width="104px" bind:value={interval} error={errors.interval} onchange={(v) => save("interval", () => actions.setScanInterval(v))} />
            {/snippet}
          </SettingRow>
        </SettingsGroup>
        <SettingsGroup title="History">
          <SettingRow label="Remember" description="How many stopped services to keep for one-click restart.">
            {#snippet children()}
              <NumberInput label="History length" labelHidden min={10} max={5000} step={10} unit="entries" width="140px" bind:value={historyLimit} error={errors.history} onchange={(v) => save("history", () => actions.setHistoryLimit(v))} />
            {/snippet}
          </SettingRow>
          <SettingRow label="Clear history" description="Removes the list of stopped services. Running services aren't affected.">
            {#snippet children()}
              {#if confirmClear}
                <Button size="sm" variant="ghost" onclick={() => (confirmClear = false)}>Cancel</Button>
                <Button size="sm" variant="danger" loading={busy.clear} onclick={() => save("clear", async () => { await actions.clearHistory(); confirmClear = false; }, "History cleared")}>Clear</Button>
              {:else}
                <Button size="sm" variant="danger-outline" onclick={() => (confirmClear = true)}>Clear history…</Button>
              {/if}
            {/snippet}
          </SettingRow>
        </SettingsGroup>
      {:else}
        <SettingsGroup title="portwise">
          <SettingRow label="Version" description={`Desktop app on ${model.platform}`}>{#snippet children()}<span class="mono ver">{model.version}</span>{/snippet}</SettingRow>
          {#if model.configDir}
            <SettingRow label="Settings folder" description="Pins, preferences and history live here." stack>
              {#snippet children()}<div class="path"><CopyValue value={model.configDir!} mono what="Path" oncopy={actions.copy} /></div>{/snippet}
            </SettingRow>
          {/if}
        </SettingsGroup>
        <p class="tip">Everything stays on this machine. portwise has no telemetry.</p>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <span class="status {status?.kind ?? ''}" aria-live="polite">
      {#if status?.kind === "saving"}<span class="dotspin"></span>{status.text}{:else if status?.kind === "saved"}<Icon name="check" size={13} />{status.text}{:else if status}<Icon name="alert" size={13} />{status.text}{:else}Changes are saved automatically.{/if}
    </span>
    <Button variant="secondary" onclick={onclose}>Done</Button>
  {/snippet}
</Dialog>

<style>
  .layout { display: grid; grid-template-columns: 188px minmax(0, 1fr); gap: var(--sp-6); min-height: 380px; }
  .nav { display: flex; flex-direction: column; gap: 2px; outline: none; }
  .nav button { display: flex; align-items: center; gap: 10px; height: var(--h-md); padding: 0 10px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); text-align: left; cursor: pointer; }
  .nav button :global(svg) { color: var(--muted); }
  .nav button:hover { background: var(--row-hover); color: var(--text); }
  .nav button.on { background: var(--row-selected); color: var(--text); }
  .nav button.on :global(svg) { color: var(--accent); }
  .nav button:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .panel { min-width: 0; outline: none; }
  .row-in { padding: 4px 16px 16px; }
  .err { margin: calc(var(--sp-3) * -1) 0 var(--sp-4); padding-top: var(--sp-4); }
  .ro { font-size: var(--fs-body); line-height: var(--lh-body); color: var(--muted); }
  .tip { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); margin: var(--sp-4) 0 0; }
  .ver { font-size: var(--fs-body); line-height: var(--lh-body); }
  .path { width: 100%; padding: 6px 10px; border-radius: var(--r-md); background: var(--surface-2); border: 1px solid var(--border); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .status { margin-right: auto; display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .status.saved { color: var(--ok); }
  .status.error { color: var(--danger); }
  .dotspin { width: 10px; height: 10px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 0.7s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) {
    .layout { grid-template-columns: 1fr; gap: var(--sp-4); min-height: 0; }
    .nav { flex-direction: row; overflow-x: auto; scrollbar-width: none; border-bottom: 1px solid var(--border); padding-bottom: var(--sp-2); }
    .nav button { flex: none; }
  }
</style>
