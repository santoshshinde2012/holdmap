<script lang="ts">
  // Read-only look at another machine's ports over SSH (agentless: ss + ps).
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import TextField from "./ui/TextField.svelte";
  import Callout from "./ui/Callout.svelte";
  import CopyValue from "./ui/CopyValue.svelte";
  import Icon from "./Icon.svelte";
  import type { Snapshot } from "../lib/types";
  import { validateHost } from "../lib/validate";
  import { title } from "../lib/format";

  let { recent = [], onscan, oncopy, onclose }: { recent?: string[]; onscan: (host: string) => Promise<Snapshot>; oncopy: (text: string, what: string) => void; onclose: () => void } = $props();

  let host = $state("");
  let touched = $state(false);
  let loading = $state(false);
  let failure = $state<string | null>(null);
  let result = $state<{ host: string; snap: Snapshot } | null>(null);
  let input: HTMLInputElement | undefined = $state();
  const error = $derived(touched ? validateHost(host) : null);

  async function scan(h = host) {
    host = h;
    touched = true;
    if (validateHost(h)) { input?.focus(); return; }
    loading = true; failure = null;
    try { result = { host: h.trim(), snap: await onscan(h.trim()) }; }
    catch (e) { failure = String(e); result = null; }
    finally { loading = false; }
  }
  const listening = $derived(result ? result.snap.entries.filter((e) => e.state === "listen" || e.protocol === "udp").sort((a, b) => a.port - b.port) : []);
</script>

<Dialog title="Remote host" description="See another machine's ports over SSH. Read-only, nothing is installed there." icon="server" size="lg" {onclose} initialFocus="#remote-host">
  <form class="form" onsubmit={(e) => { e.preventDefault(); scan(); }} novalidate>
    <div class="line">
      <TextField
        id="remote-host"
        label="SSH host"
        icon="terminal"
        placeholder="devbox or user@10.0.0.5:2222"
        mono
        clearable
        bind:value={host}
        bind:input
        {error}
        hint="Uses your ~/.ssh/config and keys (BatchMode — no password prompts)."
        autocomplete="off"
        spellcheck="false"
        autocapitalize="off"
        onblur={() => host && (touched = true)}
      />
      <Button type="submit" variant="primary" icon="radar" {loading} loadingText="Connecting…" class="go">Scan</Button>
    </div>
    {#if recent.length}
      <div class="recent" role="group" aria-label="Recent hosts">
        <span>Recent</span>
        {#each recent as h (h)}<button type="button" class="rh" onclick={() => scan(h)} disabled={loading}><Icon name="history" size={11} />{h}</button>{/each}
      </div>
    {/if}
  </form>

  {#if failure}
    <div class="out"><Callout tone="danger" title="Couldn't read {host.trim()}">
      <p class="selectable">{failure}</p>
      <p class="sub">Check that <code>ssh {host.trim()}</code> works in a terminal without a password prompt.</p>
    </Callout></div>
  {:else if loading}
    <div class="out sk" aria-busy="true">{#each Array(4) as _}<div class="shimmer" style="height:34px"></div>{/each}</div>
  {:else if result}
    <div class="out">
      <div class="rhead"><Icon name="server" size={14} /><b>{result.host}</b><span>{listening.length} listening · scanned in {result.snap.scan_ms} ms</span>
        <span class="cli"><CopyValue value="portwise ssh {result.host}" mono what="Command" {oncopy} /></span></div>
      {#if listening.length}
        <div class="tbl" role="table" aria-label="Ports on {result.host}">
          <div class="tr th" role="row"><span role="columnheader">Port</span><span role="columnheader">Process</span><span role="columnheader">User</span><span role="columnheader">Address</span></div>
          {#each listening as e (e.id)}
            <div class="tr" role="row">
              <span role="cell" class="p">{e.port}<em>{e.protocol}</em></span>
              <span role="cell" class="n">{title(e)}{#if e.process}<em>PID {e.process.pid}</em>{/if}</span>
              <span role="cell" class="u">{e.user ?? "—"}</span>
              <span role="cell" class="a" class:exp={e.exposure === "all_interfaces"}>{e.addresses.join(", ")}</span>
            </div>
          {/each}
        </div>
      {:else}<p class="none">Nothing is listening there.</p>{/if}
    </div>
  {/if}
</Dialog>

<style>
  .form { display: grid; gap: var(--sp-3); padding-top: var(--sp-1); }
  .line { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-2); align-items: start; }
  .line :global(.go) { margin-top: 26px; }
  .recent { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: var(--fs-xs); color: var(--muted); }
  .recent > span { margin-right: 2px; }
  .rh { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 8px; border-radius: var(--r-full); border: 1px solid var(--input-border); background: var(--surface); color: var(--text-2); font: inherit; font-family: var(--mono); font-size: 11px; cursor: pointer; }
  .rh:hover:not(:disabled) { background: var(--surface-2); color: var(--text); }
  .rh:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .out { margin-top: var(--sp-5); }
  .sk { display: grid; gap: 6px; }
  .sub { margin-top: 6px !important; color: var(--muted); }
  .rhead { display: flex; align-items: center; gap: 8px; margin-bottom: var(--sp-3); font-size: var(--fs-sm); min-width: 0; }
  .rhead > :global(svg) { color: var(--muted); }
  .rhead span { color: var(--muted); font-size: var(--fs-xs); }
  .cli { margin-left: auto; max-width: 45%; font-size: 11px; }
  .tbl { border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; max-height: 300px; overflow-y: auto; }
  .tr { display: grid; grid-template-columns: 90px minmax(0, 1.4fr) minmax(0, 0.6fr) minmax(0, 1fr); gap: 12px; align-items: center; padding: 0 12px; min-height: 36px; font-size: var(--fs-sm); }
  .tr + .tr { border-top: 1px solid var(--border); }
  .th { position: sticky; top: 0; background: var(--surface-2); min-height: 30px; font-size: var(--fs-2xs); font-weight: 650; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .tr span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p { font-family: var(--mono); font-weight: 700; }
  em { font-style: normal; font-weight: 500; color: var(--muted); font-size: 10.5px; margin-left: 6px; text-transform: uppercase; font-family: var(--font); }
  .n em { text-transform: none; font-family: var(--mono); }
  .u { color: var(--text-2); }
  .a { font-family: var(--mono); font-size: 11.5px; color: var(--muted); }
  .a.exp { color: var(--warn); }
  .none { color: var(--muted); }
</style>
