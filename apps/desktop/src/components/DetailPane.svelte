<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Explanation, PortEntry } from "../lib/types";
  import { canOpen, command, describeStep, humanBytes, tildify, title, tone, uptime, url } from "../lib/format";

  let {
    entry,
    explanation,
    loading,
    busy,
    onstop,
    onkill,
    onopen,
    oncopy,
  }: {
    entry: PortEntry | null;
    explanation: Explanation | null;
    loading: boolean;
    busy: boolean;
    onstop: () => void;
    onkill: () => void;
    onopen: () => void;
    oncopy: (text: string, what: string) => void;
  } = $props();

  const plan = $derived(explanation?.plan ?? null);
  const blocked = $derived(plan?.blocked ?? null);
</script>

<aside class="pane" aria-label="Port details" aria-live="polite">
  {#if !entry}
    <div class="placeholder">
      <div class="ph-icon"><Icon name="radar" size={28} /></div>
      <h3>Pick a port</h3>
      <p>Select a row to see who owns it, why it's busy, and exactly what stopping it will do.</p>
    </div>
  {:else}
    <header class="head">
      <div class="big mono">:{entry.port}<span class="proto">{entry.protocol}</span></div>
      <div class="who">
        <h2>{title(entry)}</h2>
        <div class="tags">
          {#if entry.framework}<span class="badge tone-{tone(entry)}"><span class="dot"></span>{entry.framework.name}</span>{/if}
          {#if entry.project?.git_branch}<span class="badge"><Icon name="branch" size={11} />{entry.project.git_branch}</span>{/if}
          {#if entry.exposure === "all_interfaces"}<span class="badge tone-amber"><Icon name="globe" size={11} />Network-exposed</span>{/if}
          {#if entry.protected}<span class="badge"><Icon name="lock" size={11} />Protected</span>{/if}
        </div>
      </div>
    </header>

    <div class="actions">
      {#if entry.process || entry.container}
        <button class="btn danger" onclick={onstop} disabled={busy || !!blocked}>
          <Icon name="stop" size={12} /> Stop <kbd class="k">⌫</kbd>
        </button>
        {#if entry.process && !entry.container}
          <button class="btn" onclick={onkill} disabled={busy || !!blocked} title="Skip SIGTERM and kill immediately (⇧⌫)">
            <Icon name="zap" size={13} /> Force kill
          </button>
        {/if}
      {/if}
      {#if canOpen(entry)}
        <button class="btn" onclick={onopen}><Icon name="external" size={13} /> Open</button>
        <button class="icon-btn" title="Copy URL" aria-label="Copy URL" onclick={() => oncopy(url(entry), "URL")}><Icon name="copy" size={14} /></button>
      {/if}
    </div>

    <div class="scroll">
      {#if loading && !explanation}
        <div class="skeleton" style="width: 90%"></div>
        <div class="skeleton" style="width: 70%"></div>
      {:else if explanation}
        <section class="why">
          <p class="headline selectable">{explanation.headline}</p>
          {#if blocked}
            <div class="callout warn" role="note">
              <Icon name={blocked.kind === "needs_elevation" ? "lock" : "shield"} size={15} />
              <div>
                <strong>{blocked.kind === "needs_elevation" ? "Needs administrator rights" : blocked.kind === "os_service" ? "This is an OS feature" : blocked.kind === "nothing_to_stop" ? "Nothing to stop" : "Protected process"}</strong>
                <p class="selectable">{blocked.message}</p>
              </div>
            </div>
          {:else}
            <div class="callout tip">
              <Icon name="sparkles" size={15} />
              <p class="selectable">{explanation.recommendation}</p>
            </div>
          {/if}
        </section>

        {#if entry.process}
          <section>
            <h4>Process</h4>
            <dl>
              <dt>Name</dt><dd>{entry.process.name} <span class="muted">PID {entry.process.pid}</span></dd>
              {#if entry.user}<dt>User</dt><dd>{entry.user}{entry.is_mine ? "" : " (not you)"}</dd>{/if}
              {#if uptime(entry)}<dt>Uptime</dt><dd>{uptime(entry)}</dd>{/if}
              {#if entry.process.memory_bytes}<dt>Memory</dt><dd>{humanBytes(entry.process.memory_bytes)}</dd>{/if}
              <dt>Command</dt>
              <dd class="cmd">
                <code class="selectable">{command(entry)}</code>
                <button class="icon-btn tiny" aria-label="Copy command" title="Copy" onclick={() => oncopy(command(entry), "Command")}><Icon name="copy" size={12} /></button>
              </dd>
              {#if entry.process.cwd}<dt>Directory</dt><dd class="mono selectable path">{tildify(entry.process.cwd)}</dd>{/if}
            </dl>
          </section>
        {/if}

        {#if entry.project}
          <section>
            <h4>Project</h4>
            <dl>
              <dt>Name</dt><dd>{entry.project.name}</dd>
              <dt>Path</dt><dd class="mono selectable path">{tildify(entry.project.root)}</dd>
              <dt>Detected by</dt><dd class="mono">{entry.project.kind}</dd>
              {#if entry.project.git_branch}<dt>Branch</dt><dd class="mono">{entry.project.git_branch}</dd>{/if}
            </dl>
          </section>
        {/if}

        {#if entry.container}
          <section>
            <h4>Container</h4>
            <dl>
              <dt>Name</dt><dd>{entry.container.name}</dd>
              <dt>Image</dt><dd class="mono">{entry.container.image}</dd>
              <dt>Runtime</dt><dd>{entry.container.runtime}</dd>
              <dt>Mapping</dt><dd class="mono">{entry.port} → {entry.container.private_port}</dd>
              {#if entry.container.compose_project}<dt>Compose</dt><dd class="mono">{entry.container.compose_project}/{entry.container.compose_service}</dd>{/if}
            </dl>
          </section>
        {/if}

        <section>
          <h4>Network</h4>
          <dl>
            <dt>Address</dt><dd class="mono selectable">{entry.addresses.join(", ")}</dd>
            <dt>State</dt><dd class="mono">{entry.state}</dd>
          </dl>
          {#if entry.exposure === "all_interfaces"}
            <div class="callout warn subtle"><Icon name="globe" size={14} /><p>Reachable from other devices on your network. Bind to <code>127.0.0.1</code> if that isn't intended.</p></div>
          {/if}
        </section>

        {#if explanation.details.length}
          <section>
            <h4>Why it's busy</h4>
            <ul class="details selectable">
              {#each explanation.details as d}<li>{d}</li>{/each}
            </ul>
          </section>
        {/if}

        {#if plan && !blocked && plan.steps.length}
          <section>
            <h4>What “Stop” will do <span class="badge risk-{plan.risk}">{plan.risk} risk</span></h4>
            <ol class="steps">
              {#each plan.steps as s}<li>{describeStep(s)}</li>{/each}
            </ol>
            {#each plan.warnings as w}<div class="callout warn subtle"><Icon name="alert" size={14} /><p>{w}</p></div>{/each}
          </section>
        {/if}

        {#if explanation.commands.length}
          <section>
            <h4>From the terminal</h4>
            {#each explanation.commands as c}
              <div class="shell">
                <span class="prompt">$</span><code class="selectable">{c}</code>
                <button class="icon-btn tiny" aria-label="Copy command" title="Copy" onclick={() => oncopy(c, "Command")}><Icon name="copy" size={12} /></button>
              </div>
            {/each}
          </section>
        {/if}
      {/if}
    </div>
  {/if}
</aside>

<style>
  .pane { display: flex; flex-direction: column; min-height: 0; height: 100%; background: var(--surface); border-left: 1px solid var(--border); }
  .placeholder { margin: auto; text-align: center; max-width: 260px; color: var(--muted); padding: 24px; }
  .placeholder h3 { color: var(--text); margin: 12px 0 4px; font-size: 14px; }
  .placeholder p { margin: 0; }
  .ph-icon { width: 56px; height: 56px; margin: 0 auto; display: grid; place-items: center; border-radius: 16px; background: var(--accent-soft); color: var(--accent); }
  .head { display: flex; gap: 14px; align-items: center; padding: 18px 20px 10px; }
  .big { font-size: 26px; font-weight: 700; letter-spacing: -0.02em; display: flex; align-items: baseline; gap: 6px; }
  .big .proto { font-size: 10px; text-transform: uppercase; color: var(--faint); letter-spacing: 0.08em; }
  .who { min-width: 0; }
  .who h2 { margin: 0 0 5px; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tags { display: flex; flex-wrap: wrap; gap: 5px; }
  .actions { display: flex; gap: 8px; padding: 4px 20px 14px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .actions .k { background: rgb(255 255 255 / 0.18); color: inherit; border-color: transparent; height: 16px; }
  .scroll { overflow-y: auto; padding: 6px 20px 24px; flex: 1; }
  section { padding: 12px 0 4px; }
  h4 { margin: 0 0 8px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.07em; color: var(--muted); font-weight: 650; display: flex; align-items: center; gap: 8px; }
  .headline { font-size: 13.5px; margin: 4px 0 10px; color: var(--text); line-height: 1.5; }
  dl { display: grid; grid-template-columns: 86px minmax(0, 1fr); gap: 6px 10px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .muted { color: var(--muted); }
  .path { color: var(--text-2); }
  .cmd { display: flex; gap: 6px; align-items: flex-start; }
  .cmd code { background: var(--surface-2); padding: 4px 7px; border-radius: 6px; flex: 1; overflow-wrap: anywhere; max-height: 72px; overflow-y: auto; }
  .tiny { width: 24px; height: 24px; }
  .callout { display: flex; gap: 10px; padding: 10px 12px; border-radius: var(--radius); font-size: 12.5px; align-items: flex-start; margin: 6px 0; }
  .callout p { margin: 0; }
  .callout strong { display: block; margin-bottom: 2px; }
  .callout.tip { background: var(--accent-soft); color: var(--text); }
  .callout.tip :global(svg) { color: var(--accent); margin-top: 1px; }
  .callout.warn { background: var(--warn-soft); color: var(--text); }
  .callout.warn :global(svg) { color: var(--warn); margin-top: 1px; }
  .callout.subtle { padding: 8px 10px; font-size: 12px; }
  .details { margin: 0; padding-left: 16px; color: var(--text-2); display: grid; gap: 4px; }
  .steps { margin: 0; padding-left: 0; list-style: none; counter-reset: s; display: grid; gap: 6px; }
  .steps li { counter-increment: s; display: flex; gap: 10px; color: var(--text-2); }
  .steps li::before { content: counter(s); flex: none; width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 700; background: var(--surface-2); color: var(--muted); }
  .shell { display: flex; align-items: center; gap: 8px; background: var(--surface-2); border-radius: 8px; padding: 4px 4px 4px 10px; margin-bottom: 6px; }
  .shell code { flex: 1; overflow-x: auto; white-space: nowrap; }
  .prompt { color: var(--faint); font-family: var(--mono); }
  .risk-low { color: var(--ok); background: var(--ok-soft); }
  .risk-medium { color: var(--warn); background: var(--warn-soft); }
  .risk-high { color: var(--danger); background: var(--danger-soft); }
  .skeleton { height: 12px; border-radius: 6px; margin: 14px 0; background: linear-gradient(90deg, var(--surface-2), var(--surface-3), var(--surface-2)); background-size: 200% 100%; animation: shimmer 1.3s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
