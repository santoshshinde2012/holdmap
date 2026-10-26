<script lang="ts">
  // Settings → Analyzers → Scheduled scans. Off by default. Turning it on (or widening it) shows
  // the plan first and needs an explicit confirm. Scans run only while holdmap is open.
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import Dialog from "../ui/Dialog.svelte";
  import * as api from "../../lib/api";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { scheduler } from "../../lib/schedule-driver.svelte";
  import { DEFAULT_SCHEDULE, INTERVALS, hourLabel, intervalLabel, requestFrom, untilLabel, widens, type ScheduleRequest, type SchedulePlan, type ScheduleStatus } from "../../lib/schedule";

  let status = $state<ScheduleStatus | null>(null);
  let draft = $state<ScheduleRequest>(requestFrom(DEFAULT_SCHEDULE));
  let plan = $state<SchedulePlan | null>(null);
  let confirming = $state<ScheduleRequest | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const hours = Array.from({ length: 24 }, (_, h) => h);
  const mcpIds = $derived(new Set(insightsStore.analyzers.filter((a) => a.kind === "mcp").map((a) => a.id)));
  const eligible = $derived(plan?.analyzers.filter((a) => !a.blocked).map((a) => a.id) ?? []);

  $effect(() => { void load(); });
  async function load() {
    try { status = await api.scheduleStatus(); draft = requestFrom(status.schedule); } catch (e) { error = msg(e); }
  }
  // Re-plan whenever the interval or MCP opt-ins change, so blocked analyzers show why.
  $effect(() => {
    const { every_hours, mcp_opt_in, skip_on_battery } = draft;
    api.schedulePlan(every_hours, [...mcp_opt_in], skip_on_battery).then((p) => (plan = p), (e) => (error = msg(e)));
  });
  const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));
  const toggle = (list: string[], id: string, on: boolean) => (on ? [...new Set([...list, id])] : list.filter((x) => x !== id));

  function save(next: ScheduleRequest) {
    const req = { ...next, analyzers: next.analyzers.filter((a) => eligible.includes(a)) };
    if (status && widens(status.schedule, req)) { confirming = req; return; }
    void apply(req);
  }
  async function apply(req: ScheduleRequest) {
    busy = true; error = null;
    try { status = await api.setSchedule(req); draft = requestFrom(status.schedule); confirming = null; await scheduler.driver?.refresh(); }
    catch (e) { error = msg(e); }
    finally { busy = false; }
  }
  const dirty = $derived(!!status && JSON.stringify(requestFrom(status.schedule)) !== JSON.stringify(draft));
</script>

<section class="sched" aria-labelledby="sched-h">
  <h4 id="sched-h">Scheduled scans</h4>
  <p class="sub">
    {#if status?.schedule.enabled}
      On · {intervalLabel(status.schedule.every_hours).toLowerCase()} · next {untilLabel(status.next_ms, Date.now())}{#if status.power === "battery" && status.schedule.skip_on_battery} · paused on battery{/if}
    {:else}
      Off. Nothing runs unless you start it.
    {/if}
  </p>
  {#if scheduler.driver?.skipped}<p class="sub">Last scheduled scan skipped: {scheduler.driver.skipped}</p>{/if}

  <div class="grid">
    <label for="sched-every">How often</label>
    <select id="sched-every" bind:value={draft.every_hours}>{#each INTERVALS as i (i.hours)}<option value={i.hours}>{i.label}</option>{/each}</select>

    <span id="sched-which">Analyzers</span>
    <div class="col" role="group" aria-labelledby="sched-which">
      {#each plan?.analyzers ?? [] as a (a.id)}
        <Checkbox label={a.name} description={a.blocked ?? (a.starts_mcp_server ? "Starts its MCP server for each scheduled scan." : undefined)} disabled={!!a.blocked} checked={draft.analyzers.includes(a.id)} onchange={(on) => (draft.analyzers = toggle(draft.analyzers, a.id, on))} />
        {#if mcpIds.has(a.id)}
          <span class="indent"><Checkbox label="Allow {a.name} on a schedule" checked={draft.mcp_opt_in.includes(a.id)} onchange={(on) => { draft.mcp_opt_in = toggle(draft.mcp_opt_in, a.id, on); if (!on) draft.analyzers = toggle(draft.analyzers, a.id, false); }} /></span>
        {/if}
      {:else}
        <span class="sub">Turn on an analyzer first.</span>
      {/each}
    </div>

    <span>Power</span>
    <Checkbox label="Skip on battery" description="If holdmap can't tell, scans run." bind:checked={draft.skip_on_battery} />

    <span>Notifications</span>
    <div class="col">
      <Checkbox label="Notify me" description="Only for new high or critical findings and changed skills, at most once an hour." bind:checked={draft.notify} />
      <span class="quiet">
        <label for="sched-qs">Quiet hours</label>
        <select id="sched-qs" bind:value={draft.quiet.start} disabled={!draft.notify}>{#each hours as h (h)}<option value={h}>{hourLabel(h)}</option>{/each}</select>
        <label for="sched-qe">to</label>
        <select id="sched-qe" bind:value={draft.quiet.end} disabled={!draft.notify}>{#each hours as h (h)}<option value={h}>{hourLabel(h)}</option>{/each}</select>
      </span>
    </div>
  </div>

  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  <div class="acts">
    {#if status?.schedule.enabled}
      <Button size="sm" variant="primary" disabled={!dirty || !draft.analyzers.length} loading={busy} onclick={() => save({ ...draft, enabled: true })}>Save</Button>
      <Button size="sm" variant="secondary" loading={busy} onclick={() => apply({ ...draft, enabled: false })}>Turn off</Button>
    {:else}
      <Button size="sm" variant="primary" icon="clock" disabled={!draft.analyzers.some((a) => eligible.includes(a))} onclick={() => save({ ...draft, enabled: true })}>Review plan and turn on…</Button>
    {/if}
  </div>
</section>

{#if confirming && plan}
  {@const chosen = plan.analyzers.filter((a) => confirming!.analyzers.includes(a.id))}
  <Dialog title="Turn on scheduled scans?" description="Here's exactly what will run." icon="clock" size="md" dismissable={!busy} onclose={() => (confirming = null)}>
    <dl class="plan">
      <div><dt>How often</dt><dd>{intervalLabel(confirming.every_hours)}</dd></div>
      <div><dt>Analyzers</dt><dd>{chosen.map((a) => a.name + (a.starts_mcp_server ? " (starts its MCP server)" : "")).join(", ")}</dd></div>
      <div><dt>Network</dt><dd>{plan.network === "none" ? "None. Only offline analyzers run on a schedule." : plan.network}</dd></div>
      <div><dt>Also</dt><dd>Re-checks skill fingerprints on this computer (names, sizes and hashes; nothing is sent).</dd></div>
      <div><dt>When</dt><dd>{plan.runs_when}</dd></div>
      <div><dt>Notifications</dt><dd>{confirming.notify ? `${plan.notifies} Quiet ${hourLabel(confirming.quiet.start)}–${hourLabel(confirming.quiet.end)}.` : "Off."}</dd></div>
    </dl>
    {#snippet footer()}
      <Button variant="secondary" disabled={busy} onclick={() => (confirming = null)}>Cancel</Button>
      <Button variant="primary" loading={busy} onclick={() => apply({ ...confirming!, confirmed: true })}>Turn on</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .sched { display: grid; gap: var(--sp-2); margin-top: var(--sp-4); padding: var(--sp-3); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  h4 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .grid { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 10px 16px; align-items: start; font-size: var(--fs-body-sm); }
  @media (max-width: 520px) { .grid { grid-template-columns: minmax(0, 1fr); gap: 4px; } }
  .col { display: grid; gap: 6px; min-width: 0; }
  .indent { padding-left: 24px; }
  .quiet { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; font-size: var(--fs-caption); color: var(--muted); }
  select { height: 28px; padding: 0 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body-sm); justify-self: start; }
  select:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  select:disabled { opacity: .55; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; }
  .plan { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 8px 16px; margin: 0; font-size: var(--fs-body-sm); }
  .plan div { display: contents; }
  .plan dt { color: var(--muted); }
  .plan dd { margin: 0; }
</style>
