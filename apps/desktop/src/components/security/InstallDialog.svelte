<script lang="ts">
  // Guided install of an analyzer's program: what will be installed, from where, into which
  // folder, what it contacts and reads, then live progress and a verified result. Nothing runs
  // until the user ticks consent and clicks Install. Copying the command stays as a fallback.
  import Dialog from "../ui/Dialog.svelte";
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import CopyValue from "../ui/CopyValue.svelte";
  import Icon from "../Icon.svelte";
  import { installDialog as d } from "../../lib/install-dialog.svelte";
  import { METHOD_LABEL, PHASE_LABEL } from "../../lib/analyzer-install";
  import { redactHome } from "../../lib/report-export";
  import { isTauri } from "../../lib/api";

  let { onreview }: { onreview?: (id: string) => void } = $props();
  const plan = $derived(d.plan);
  const s = $derived(d.state);
  const reviewing = $derived(s.phase === "review");
  let logEl = $state<HTMLElement | null>(null);
  $effect(() => { void s.log.length; logEl?.scrollTo({ top: logEl.scrollHeight }); });
  const copy = (t: string) => navigator.clipboard?.writeText(t);
</script>

{#if d.open}
  <Dialog title={plan ? `Install ${plan.name}` : "Install analyzer"} description="holdmap installs the pinned version into its own folder. Nothing runs until you agree." icon="download" size="lg" dismissable={!d.busy} closeButton={!d.busy} onclose={() => d.close()} initialFocus="#install-consent">
    {#if d.loadError}
      <Callout tone="danger" title="Couldn't prepare the install">{d.loadError}</Callout>
    {:else if !plan}
      <p class="muted" role="status">Checking what this install needs…</p>
    {:else}
      <div class="plan" data-install-plan>
        <dl class="facts">
          <div><dt>Package</dt><dd><code>{plan.package || "—"}</code></dd></div>
          <div><dt>Version</dt><dd>{plan.version || "unknown"}</dd></div>
          <div><dt>Source</dt><dd>{plan.source || "unknown"}</dd></div>
          <div><dt>Installed with</dt><dd>{METHOD_LABEL[plan.method]}{#if plan.toolchain}{#if plan.toolchain.path} · <code>{redactHome(plan.toolchain.path)}</code>{:else} · <span class="warn">not found</span>{/if}{/if}</dd></div>
          <div class="wide"><dt>Installs into</dt><dd><code>{redactHome(plan.target_dir)}</code><small>holdmap's own folder · no admin rights · nothing system-wide · your shell config isn't changed</small></dd></div>
          <div class="wide"><dt>Network during install</dt><dd>{plan.network.length ? plan.network.join(", ") : "none"}</dd></div>
          <div class="wide"><dt>Checked by</dt><dd>{plan.verification || "unknown"}</dd></div>
          <div class="wide"><dt>Once approved, each run</dt><dd>Reads: {plan.reads.length ? plan.reads.join("; ") : "nothing holdmap knows of"} · Network: {plan.run_network.length ? plan.run_network.join(", ") : "none"}</dd></div>
        </dl>

        {#if plan.installed && reviewing}
          <Callout tone="ok" size="sm" title="Already installed by holdmap">Version {plan.installed.version} · <code>{redactHome(plan.installed.binary)}</code>. Installing again replaces it.</Callout>
        {/if}

        {#if plan.blocked}
          <Callout tone="warn" title={plan.method === "manual" ? "Install it yourself" : "Can't install from here yet"}>
            {plan.blocked}
            {#snippet actions()}{#if plan.help_url}<a href={plan.help_url} target="_blank" rel="noopener noreferrer">How to install {plan.toolchain && !plan.toolchain.path ? plan.toolchain.name : plan.name}<span class="sr-only"> (opens in a new tab)</span></a>{/if}{/snippet}
          </Callout>
        {/if}

        {#if reviewing && plan.can_install}
          <Checkbox id="install-consent" bind:checked={d.agreed} label="Install {plan.name} {plan.version} into holdmap's folder" description="The package manager downloads from the hosts above and runs the package's install steps with a cleared environment and a 10-minute limit. The analyzer stays off until you approve it." />
        {/if}

        {#if !reviewing}
          <div class="progress ph-{s.phase}" role="status" aria-live="polite" data-install-status={s.phase}>
            <span class="t">{#if d.busy}<span class="spin" aria-hidden="true"></span>{:else}<Icon name={s.phase === "done" ? "check" : s.phase === "cancelled" ? "x" : "alert"} size={14} />{/if}{PHASE_LABEL[s.phase]}{#if s.phase === "done" && s.record} · version {s.record.version}{/if}</span>
            {#if s.reason}<p class="reason">{s.reason}</p>{/if}
            {#if s.phase === "done" && s.record}
              <p class="reason">Verified <code>{redactHome(s.record.binary)}</code> · SHA-256 <code title={s.record.sha256}>{s.record.sha256.slice(0, 12)}…</code>. Next: review its permissions and turn it on.</p>
            {/if}
          </div>
          {#if s.log.length}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex (a scrollable log must be reachable by keyboard) -->
            <pre class="log" bind:this={logEl} aria-label="Install log" tabindex="0">{s.log.join("\n")}</pre>
          {/if}
        {/if}

        {#if plan.fallback || plan.command.length}
          <details class="fallback">
            <summary>Prefer to run it yourself?</summary>
            <p class="muted">Copy the publisher's command and run it in your own terminal. holdmap finds the program on your PATH afterwards.</p>
            <CopyValue value={plan.fallback ?? plan.command.join(" ")} mono wrap what="Install command" oncopy={copy} />
          </details>
        {/if}
        {#if !isTauri}<p class="muted"><Icon name="info" size={12} /> Browser demo: nothing is downloaded or installed.</p>{/if}
      </div>
    {/if}

    {#snippet footer()}
      {#if d.busy}
        <Button variant="danger-outline" icon="stop" onclick={() => d.cancel()}>Cancel install</Button>
      {:else if s.phase === "done"}
        <Button variant="ghost" onclick={() => d.close()}>Close</Button>
        {#if onreview && d.id}<Button variant="primary" icon="shield" onclick={() => { const id = d.id!; d.close(); onreview(id); }}>Review and turn on…</Button>{/if}
      {:else}
        <Button variant="ghost" onclick={() => d.close()}>{reviewing ? "Not now" : "Close"}</Button>
        {#if plan?.can_install}
          <Button variant="primary" icon={reviewing ? "download" : "refresh"} disabled={!d.agreed} onclick={() => d.install()} data-install-go>{reviewing ? "Install" : "Retry"}</Button>
        {/if}
      {/if}
    {/snippet}
  </Dialog>
{/if}

<style>
  .plan { display: grid; gap: 14px; }
  .facts { margin: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px 18px; }
  .facts .wide { grid-column: 1 / -1; }
  dt { color: var(--muted); font-size: var(--fs-caption); }
  dd { margin: 2px 0 0; font-size: var(--fs-body-sm); color: var(--text); overflow-wrap: anywhere; display: grid; gap: 2px; }
  dd small { color: var(--muted); font-size: var(--fs-caption); }
  code { font-family: var(--mono); font-size: var(--fs-mono-sm); }
  .warn { color: var(--tone-amber); }
  .muted { margin: 0; color: var(--muted); font-size: var(--fs-caption); display: flex; gap: 6px; align-items: center; }
  .progress { display: grid; gap: 4px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); }
  .t { display: inline-flex; align-items: center; gap: 8px; font-weight: var(--fw-semibold); font-size: var(--fs-body-sm); }
  .ph-done .t { color: var(--ok); } .ph-failed .t { color: var(--danger); }
  .reason { margin: 0; font-size: var(--fs-body-sm); color: var(--text-2); overflow-wrap: anywhere; }
  .log { margin: 0; max-height: 160px; overflow: auto; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface-3); color: var(--text-2); font-family: var(--mono); font-size: var(--fs-mono-sm); white-space: pre-wrap; overflow-wrap: anywhere; }
  .log:focus-visible { outline: 2px solid var(--ring); }
  .fallback summary { cursor: pointer; font-size: var(--fs-body-sm); color: var(--text-2); }
  .fallback { display: grid; gap: 6px; }
  .fallback[open] { gap: 8px; }
  .spin { width: 11px; height: 11px; border: 2px solid var(--accent-soft); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  a { color: var(--accent); }
</style>
