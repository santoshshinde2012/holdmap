<script lang="ts">
  // Add your own analyzer: a preset for a well-known local scanner, any MCP server (a command
  // holdmap starts, a URL it connects to, or one of your agents' configured servers), or a
  // manifest file. holdmap validates it and keeps it off until its permissions are approved.
  // For MCP servers the next step lists the tools and maps their inputs. Steps: Source → Tools →
  // Review; fields are checked inline as you type (lib/add-analyzer.ts).
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import TextField from "../ui/TextField.svelte";
  import Field from "../ui/Field.svelte";
  import SourcePicker, { type SourceOption } from "./add/SourcePicker.svelte";
  import ReviewCard from "./add/ReviewCard.svelte";
  import McpTools from "./McpTools.svelte";
  import Stepper from "../ui/Stepper.svelte";
  import { ADD_STEPS, draftErrors, idFromName, type AddStep } from "../../lib/add-analyzer-flow";
  import * as api from "../../lib/api";
  import type { ConfiguredServer, McpDraft } from "../../lib/analyzers";
  import type { PresetView } from "../../lib/analyzer-presets";

  type Mode = "preset" | "command" | "url" | "agents" | "manifest";
  const ALL: (SourceOption & { value: Mode })[] = [
    { value: "preset", label: "Presets", icon: "box", desc: "Well-known local scanners" },
    { value: "command", label: "Command", icon: "terminal", desc: "An MCP server holdmap starts" },
    { value: "url", label: "Local URL", icon: "link", desc: "An HTTP MCP server you run" },
    { value: "agents", label: "From your agents", icon: "bot", desc: "Copy an agent's MCP server" },
    { value: "manifest", label: "Manifest", icon: "code", desc: "Paste a TOML manifest" },
  ];
  // The Security gallery already shows presets as cards, so it opens this form without them.
  let { onadded, oncancel, ondone, onreview, demo = false, modes = ALL.map((m) => m.value), startOpen = false }:
    { onadded?: (id: string) => void; oncancel?: () => void; ondone?: () => void; onreview?: (id: string) => void; demo?: boolean; modes?: Mode[]; startOpen?: boolean } = $props();
  const options = $derived(ALL.filter((m) => modes.includes(m.value)));
  // svelte-ignore state_referenced_locally
  let open = $state(startOpen);
  // svelte-ignore state_referenced_locally
  let mode = $state<Mode>(modes[0] ?? "preset");
  let presets = $state<PresetView[] | null>(null);
  let id = $state("local.");
  let name = $state("");
  let command = $state("");
  let args = $state("");
  let url = $state("http://127.0.0.1:8765/mcp");
  let auth = $state("");
  let keys = $state("");
  let text = $state("");
  let servers = $state<ConfiguredServer[] | null>(null);
  let picked = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let added = $state<{ id: string; name: string } | null>(null);
  /** Set once the analyzer exists: the Review step. */
  let done = $state<{ id: string; name: string; tools: boolean; source: string } | null>(null);
  let touched = $state<Record<string, boolean>>({});
  let idEdited = $state(false);
  const step = $derived<AddStep>(done ? "review" : added ? "tools" : "source");

  const remote = $derived.by(() => {
    try { const h = new URL(url).hostname.replace(/^\[|\]$/g, ""); return !(h === "localhost" || h === "::1" || /^127\./.test(h)); } catch { return false; }
  });
  const server = $derived(servers?.find((s) => s.id === picked) ?? null);
  const names = (s: string) => s.split(/[\s,]+/).map((x) => x.trim()).filter(Boolean);

  $effect(() => {
    if (open && mode === "preset" && presets === null) api.analyzerPresets().then((p) => (presets = p), (e) => { presets = []; error = String(e); });
  });
  $effect(() => {
    if (mode === "agents" && servers === null) api.configuredMcpServers().then((s) => (servers = s), (e) => { servers = []; error = String(e); });
  });

  function pick(s: ConfiguredServer) {
    picked = s.id;
    if (s.draft) { id = s.draft.id; name = s.draft.name; }
  }

  function draft(): McpDraft {
    const base = { id: id.trim(), name: name.trim() || id.trim() };
    if (mode === "url") return { ...base, transport: "http", url: url.trim(), auth_secret: auth.trim() || null };
    if (mode === "agents" && server?.draft) return { ...server.draft, ...base, secrets: [...(server.draft.secrets ?? []), ...names(keys)] };
    return { ...base, transport: "stdio", command: command.trim(), args: args.split("\n").map((a) => a.trim()).filter(Boolean), secrets: names(keys) };
  }

  async function add() {
    touched = { ...touched, __submit: true };
    if (busy || !ready) return;
    busy = true;
    error = null;
    try {
      if (mode === "manifest") {
        // A manifest already names its tools; edit them later with Tools….
        const view = await api.addAnalyzer(text);
        text = "";
        done = { id: view.id, name: view.name, tools: false, source: "Manifest" };
        onadded?.(view.id);
      } else {
        const view = await api.addMcpAnalyzer(draft());
        added = { id: view.id, name: view.name };
      }
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  async function addPreset(p: PresetView) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      const view = await api.addAnalyzerPreset(p.id);
      presets = null;
      open = false;
      onadded?.(view.id);
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  /** Tools saved (or skipped): move to Review. */
  function toolsDone(saved: boolean) {
    const a = added;
    added = null;
    if (!a) return;
    done = { id: a.id, name: a.name, tools: saved, source: SOURCE_TEXT[mode] };
    onadded?.(a.id);
  }
  const errors = $derived(draftErrors(mode, { id, command, url, keys: mode === "url" ? auth : keys, manifest: text, serverPicked: !!server?.draft }));
  const SOURCE_TEXT: Record<Mode, string> = { preset: "Preset", command: "Command (stdio), started in the sandbox", url: "Local URL (Streamable HTTP)", agents: "Copy of an agent's MCP server", manifest: "Manifest" };
  const HELP: Record<Mode, string> = {
    preset: "Run on this computer without an LLM or API key. You install the tool; holdmap adds it, off until you approve it.",
    command: "Any stdio MCP server. holdmap starts it in the sandbox with a cleared environment, only for scans you start.",
    url: "A Streamable HTTP MCP server you run. holdmap connects only for scans you start; it isn't sandboxed.",
    agents: "holdmap starts its own copy. It gets none of your agent's settings or keys.",
    manifest: "A manifest for a local command-line or MCP scanner.",
  };
  const ready = $derived(Object.keys(errors).length === 0);
  const shown = (k: string) => (touched[k] || touched.__submit) && errors[k] ? errors[k] : null;
  const touch = (k: string) => () => (touched = { ...touched, [k]: true });
  function nameInput() { if (!idEdited) id = idFromName(name); }
  function finish() { done = null; touched = {}; idEdited = false; open = startOpen; ondone?.(); }
</script>

<div class="add" class:panel={open} data-testid="add-analyzer">
  {#if !open}
    <Button size="sm" variant="ghost" icon="plus" onclick={() => (open = true)}>Add your own analyzer…</Button>
  {:else}
  <header class="head">
    <div class="title"><h3>Add your own analyzer</h3><p>Off until you approve it · runs sandboxed</p></div>
    {#if mode !== "preset"}<Stepper label="Add analyzer steps" steps={mode === "manifest" ? ADD_STEPS.filter((x) => x.id !== "tools") : ADD_STEPS} current={step} />{/if}
  </header>
  {#if done}
    <ReviewCard name={done.name} id={done.id} source={done.source} tools={done.tools ? "Chosen and pinned" : mode === "manifest" ? null : "Not chosen yet (Tools… in Settings)"}
      ondone={finish} onreview={onreview ? () => { const rid = done!.id; finish(); onreview(rid); } : undefined} />
  {:else if added}
    <McpTools id={added.id} name={added.name} onsaved={() => toolsDone(true)} oncancel={() => toolsDone(false)} />
  {:else}
    <SourcePicker bind:value={mode} label="How to add it" {options} />
    <p class="help">{HELP[mode]}{demo ? " Browser demo: nothing is installed, started or contacted." : ""}</p>
    {#if mode === "preset"}
      {#if presets === null}<p class="help">Loading presets…</p>
      {:else}
        <ul class="presets" aria-label="Analyzer presets">
          {#each presets as p (p.id)}
            <li>
              <span class="logo" aria-hidden="true">{p.name.charAt(0)}</span>
              <div class="pm">
                <div class="ph"><span class="nm">{p.name}</span><span class="found" class:yes={p.installed}>{p.installed ? "Installed" : "Not installed"}</span></div>
                <p class="sub">{p.publisher} · {p.license} · {p.network.length ? `contacts ${p.network.join(", ")}` : "no network"}</p>
                <p class="sub">{p.summary}</p>
                {#if !p.installed && p.install}<p class="sub">Install: <code>{p.install}</code></p>{/if}
              </div>
              <div class="pa">
                {#if p.homepage}<a class="sub" href={p.homepage} target="_blank" rel="noopener noreferrer">Project page<span class="sr-only"> for {p.name} (opens in a new tab)</span></a>{/if}
                {#if p.added}<span class="sub">Added</span>
                {:else}<Button size="sm" variant="secondary" loading={busy} onclick={() => addPreset(p)} aria-label="Add {p.name}">Add</Button>{/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <div class="form">
        {#if mode !== "manifest"}
          <TextField label="Name" bind:value={name} placeholder="My scanner" oninput={nameInput} />
          <TextField label="Id" bind:value={id} mono placeholder="local.my-scanner" spellcheck="false" oninput={() => (idEdited = true)} onblur={touch("id")} error={shown("id")} hint="Reverse-DNS, suggested from the name" />
        {/if}
        {#if mode === "command"}
          <div class="wide"><TextField label="Command" bind:value={command} mono placeholder="/usr/local/bin/my-mcp-scanner" spellcheck="false" onblur={touch("command")} error={shown("command")} /></div>
          <Field id="add-args" label="Arguments (one per line)" optional>
            {#snippet children({ id: fid })}<textarea id={fid} rows="2" bind:value={args} spellcheck="false" placeholder="--stdio"></textarea>{/snippet}
          </Field>
          <TextField label="Keychain key names" optional bind:value={keys} mono placeholder="SCANNER_TOKEN" spellcheck="false" onblur={touch("keys")} error={shown("keys")} hint="Secrets it may read from your keychain" />
        {:else if mode === "url"}
          <div class="wide"><TextField label="URL" bind:value={url} mono spellcheck="false" onblur={touch("url")} error={shown("url")} /></div>
          <div class="wide"><TextField label="Bearer token key name" optional bind:value={auth} mono placeholder="SCANNER_TOKEN" spellcheck="false" hint="You store the token in the keychain next" /></div>
          {#if remote}<div class="wide"><Callout tone="warn" size="sm">This server is on another machine. It must use https, gets only skill names, file names and agent ids, and stays off until you explicitly allow it.</Callout></div>{/if}
        {:else if mode === "agents"}
          <div class="wide">
            {#if servers === null}<p class="help">Reading your agents' configuration…</p>
            {:else if !servers.length}<p class="help">No configured MCP servers were found.</p>
            {:else}
              <div class="servers" role="radiogroup" aria-label="Configured MCP servers">
                {#each servers as s, i (s.id + s.agent)}
                  <label class="srv" class:on={picked === s.id} class:off={!s.draft}><input type="radio" name="server" checked={picked === s.id} disabled={!s.draft} aria-describedby={s.note ? `server-note-${i}` : undefined} onchange={() => pick(s)} />
                    <span class="nm">{s.name}</span><span class="sub">{s.agent} · {s.transport}{s.draft ? "" : " · can't be added"}</span>
                    {#if s.note}<span class="sub note" id={`server-note-${i}`}>{s.note}</span>{/if}</label>
                {/each}
              </div>
              {#if shown("server")}<p class="err">{shown("server")}</p>{/if}
              {#if server?.draft}<Callout tone="warn" size="sm">{server.name} is also {server.agent}'s server. holdmap starts a separate copy for each scan; anything it does on start (signing in, syncing) happens again.</Callout>{/if}
            {/if}
          </div>
        {:else}
          <div class="wide">
            <Field id="analyzer-manifest" label="Analyzer manifest (TOML)" error={shown("manifest")}>
              {#snippet children({ id: fid, describedBy, invalid })}<textarea id={fid} rows="8" spellcheck="false" bind:value={text} placeholder={'schema = 1\nid = "example.scanner"\n…'} onblur={touch("manifest")} aria-invalid={invalid} aria-describedby={describedBy}></textarea>{/snippet}
            </Field>
          </div>
        {/if}
      </div>
    {/if}
    <footer class="foot">
      <Button size="sm" variant="ghost" onclick={() => { open = startOpen; error = null; touched = {}; oncancel?.(); }}>Cancel</Button>
      {#if mode !== "preset"}<Button size="sm" variant="primary" loading={busy} onclick={add}>{mode === "manifest" ? "Add analyzer" : "Add and choose tools…"}</Button>{/if}
    </footer>
  {/if}
  {/if}
  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
</div>

<style>
  .add { display: grid; gap: 14px; margin-top: var(--sp-3); }
  .panel { padding: 16px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); box-shadow: var(--shadow-sm); }
  .head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px 16px; padding-bottom: 14px; border-bottom: 1px solid var(--border); }
  .title { display: grid; gap: 2px; }
  h3 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .title p, .help { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .form { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px 16px; align-items: start; }
  .wide { grid-column: 1 / -1; display: grid; gap: 8px; }
  @media (max-width: 640px) { .form { grid-template-columns: minmax(0, 1fr); } }
  textarea { width: 100%; box-sizing: border-box; font-family: var(--mono); font-size: var(--fs-body-sm); padding: 8px 10px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--input-bg, var(--surface)); color: var(--text); resize: vertical; }
  textarea:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  textarea[aria-invalid="true"] { border-color: var(--danger); }
  .err { margin: 0; color: var(--danger); font-size: var(--fs-body-sm); font-weight: var(--fw-medium); }
  .foot { position: sticky; bottom: 0; z-index: 1; display: flex; justify-content: flex-end; gap: 8px; margin: 0 -16px -16px; padding: 12px 16px; border-top: 1px solid var(--border); background: var(--surface); border-radius: 0 0 var(--r-lg) var(--r-lg); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .nm { color: var(--text); font-size: var(--fs-body-sm); font-weight: var(--fw-medium); }
  .presets { list-style: none; margin: 0; padding: 0; display: grid; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .presets li { display: grid; grid-template-columns: 32px minmax(0, 1fr) auto; gap: 12px; align-items: start; padding: 12px; }
  .presets li + li { border-top: 1px solid var(--border); }
  .logo { display: grid; place-items: center; width: 32px; height: 32px; border-radius: var(--r-md); background: var(--surface-3); color: var(--text-2); font-weight: var(--fw-semibold); }
  .pm { display: grid; gap: 2px; min-width: 0; }
  .ph { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .pa { display: flex; align-items: center; gap: 10px; }
  .found { font-size: var(--fs-caption); color: var(--muted); }
  .found.yes { color: var(--ok-text, var(--ok)); }
  .servers { display: grid; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .srv { display: grid; grid-template-columns: auto minmax(0, 1fr); column-gap: 10px; align-items: center; padding: 10px 12px; cursor: pointer; }
  .srv + .srv { border-top: 1px solid var(--border); }
  .srv.on { background: var(--accent-softer); }
  .srv.off { cursor: default; opacity: 0.7; }
  .srv input { grid-row: span 3; }
  code { font-family: var(--mono); font-size: var(--fs-caption); overflow-wrap: anywhere; }
</style>
