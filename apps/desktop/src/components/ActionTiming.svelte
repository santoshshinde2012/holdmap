<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Select from "./ui/Select.svelte";
  import Field, { fieldId } from "./ui/Field.svelte";
  import { SCHEDULE_PRESETS, scheduleDeadline } from "../lib/schedules";

  let { delaySecs = $bindable(0), atMs = $bindable(null), valid = $bindable(true), disabled = false, scope = "stop", demo = false }: {
    delaySecs?: number;
    atMs?: number | null;
    valid?: boolean;
    disabled?: boolean;
    scope?: "shutdown" | "stop";
    demo?: boolean;
  } = $props();

  let timing = $state("0");
  let minutes = $state<number | undefined>(60);
  let now = $state(Date.now());
  function localDateTime(ms: number): string {
    const date = new Date(ms);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
  }
  let atTime = $state(localDateTime(Date.now() + 3_600_000));
  const inputId = fieldId("schedule-minutes");
  const dateId = fieldId("schedule-time");
  const options = [...SCHEDULE_PRESETS.map((preset) => ({ value: String(preset.value), label: preset.value ? `In ${preset.label}` : "Now" })), { value: "custom", label: "Custom delay…" }, { value: "at", label: "At a time…" }];
  const selectedAt = $derived(atTime ? new Date(atTime).getTime() : NaN);
  const atValid = $derived(Number.isFinite(selectedAt) && selectedAt - now >= 60_000 && selectedAt - now <= 86_400_000);
  const customValid = $derived(Number.isSafeInteger(minutes) && Number(minutes) >= 1 && Number(minutes) <= 1440);
  const timingError = $derived(timing === "custom" && !customValid ? "Enter a whole number between 1 and 1,440 minutes." : null);
  $effect(() => {
    atMs = timing === "at" ? selectedAt : null;
    delaySecs = timing === "at" ? Math.floor((selectedAt - untrack(() => Date.now())) / 1000) : timing === "custom" ? (customValid ? Number(minutes) * 60 : NaN) : Number(timing);
  });
  $effect(() => { valid = timing === "at" ? atValid : timing !== "custom" || customValid; });
  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
</script>

<section class="timing" aria-label="Action timing">
  <div class="controls">
    <Select label="When" bind:value={timing} {options} {disabled} width="170px" />
    {#if timing === "custom"}
      <Field id={inputId} label="Delay in minutes" error={timingError}>
        {#snippet children({ id, describedBy, invalid })}
          <input {id} type="number" min="1" max="1440" step="1" bind:value={minutes} {disabled} aria-invalid={invalid || undefined} aria-describedby={describedBy} />
        {/snippet}
      </Field>
    {:else if timing === "at"}
      <Field id={dateId} label="Local date and time" error={atValid ? null : "Choose a time between 1 minute and 24 hours from now."}>
        {#snippet children({ id, describedBy, invalid })}
          <input {id} type="datetime-local" bind:value={atTime} min={localDateTime(Math.ceil((now + 60_000) / 60_000) * 60_000)} max={localDateTime(now + 86_400_000)} {disabled} aria-invalid={invalid || undefined} aria-describedby={describedBy} />
        {/snippet}
      </Field>
    {/if}
  </div>
  {#if delaySecs > 0 && valid}
    <p class="deadline"><span>{scope === "shutdown" ? "Shutdown" : "Stop"} at</span><strong>{scheduleDeadline(atMs ?? now + delaySecs * 1000)}</strong></p>
    <p class="note">{demo ? "Simulation runs while this demo page stays open. No computer or process is changed." : "Keep Holdmap running, including in the tray, and this computer awake. Quitting clears schedules. A missed deadline or clock change requires a new review; actions are never retried automatically."}</p>
    {#if scope === "stop"}<p class="note">Only the reviewed services are scheduled. Holdmap checks their identities again before stopping them. Scheduled stops follow the reviewed stop plan and grace period. Force-stop requests and protected-process overrides are unavailable.</p>{/if}
  {/if}
</section>

<style>
  .timing { display: grid; gap: var(--sp-3); margin-bottom: var(--sp-4); padding: var(--sp-3); background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .controls { display: flex; align-items: flex-start; flex-wrap: wrap; gap: var(--sp-3); }
  input { width: 170px; max-width: 100%; box-sizing: border-box; height: var(--h-md); padding: 0 10px; border: 1px solid var(--input-border); border-radius: var(--r-md); background: var(--input-bg); color: var(--text); font: inherit; font-size: var(--fs-body); line-height: var(--lh-body); box-shadow: var(--control-shadow); }
  input:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  input[aria-invalid="true"] { border-color: var(--danger); }
  input:disabled { opacity: 0.55; cursor: not-allowed; }
  input[type="datetime-local"] { width: 250px; }
  .deadline { display: grid; gap: var(--sp-1); margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); color: var(--text); }
  .deadline span { color: var(--muted); }
  .deadline strong { font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
</style>
