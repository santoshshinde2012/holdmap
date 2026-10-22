<script lang="ts">
  // Add your own analyzer: a preset for a well-known local scanner, any MCP server (a command
  // holdmap starts, a URL it connects to, or one of your agents' configured servers), or a
  // manifest file. holdmap validates it and keeps it off until its permissions are approved.
  // For MCP servers the next step lists the tools and maps their inputs. Steps: Source → Tools →
  // Review; fields are checked inline as you type (lib/add-analyzer.ts).
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import McpTools from "./McpTools.svelte";
  import Stepper from "../ui/Stepper.svelte";
  import { ADD_STEPS, draftErrors, idFromName, type AddStep } from "../../lib/add-analyzer-flow";
  import * as api from "../../lib/api";
  import type { ConfiguredServer, McpDraft } from "../../lib/analyzers";
  import type { PresetView } from "../../lib/analyzer-presets";

  type Mode = "preset" | "command" | "url" | "agents" | "manifest";
  const ALL: { value: Mode; label: string }[] = [
    { value: "preset", label: "Presets" }, { value: "command", label: "Command" }, { value: "url", label: "Local URL" },
    { value: "agents", label: "From your agents" }, { value: "manifest", label: "Manifest" },
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
  let done = $state<{ id: string; name: string; tools: boolean } | null>(null);
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
        done = { id: view.id, name: view.name, tools: false };
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
    done = { id: a.id, name: a.name, tools: saved };
    onadded?.(a.id);
  }
  const errors = $derived(draftErrors(mode, { id, command, url, keys: mode === "url" ? auth : keys, manifest: text, serverPicked: !!server?.draft }));
  const ready = $derived(Object.keys(errors).length === 0);
  const shown = (k: string) => (touched[k] || touched.__submit) && errors[k] ? errors[k] : null;
  const touch = (k: string) => () => (touched = { ...touched, [k]: true });
  function nameInput() { if (!idEdited) id = idFromName(name); }
  function finish() { done = null; touched = {}; idEdited = false; open = startOpen; ondone?.(); }
</script>

<div class="add" data-testid="add-analyzer">
  {#if !open}
    <Button size="sm" variant="ghost" icon="plus" onclick={() => (open = true)}>Add your own analyzer…</Button>
  {:else}
  <div class="head"><h3>Add your own analyzer</h3>{#if mode !== "preset"}<Stepper label="Add analyzer steps" steps={mode === "manifest" ? ADD_STEPS.filter((x) => x.id !== "tools") : ADD_STEPS} current={step} />{/if}</div>
  {#if done}
    <div class="review" role="status">
      <p class="ok"><b>{done.name}</b> is added and <b>off</b>.</p>
      <ul class="facts">
        <li>Nothing runs until you review its permissions and turn it on.</li>
        <li>When on, it runs only when you start a scan, in the sandbox, on a private copy of each skill.</li>
        {#if done.tools}<li>Its tool list is pinned. If the server's tools change, holdmap asks you again.</li>{/if}
      </ul>
      <div class="acts">
        {#if onreview}<Button size="sm" variant="primary" icon="shield" onclick={() => { const id = done!.id; finish(); onreview(id); }}>Review and turn on…</Button>{/if}
        <Button size="sm" variant={onreview ? "ghost" : "secondary"} onclick={finish}>Done</Button>
      </div>
    </div>
  {:else if added}
    <p class="meta">Choose {added.name}'s tools.</p>
    <McpTools id={added.id} name={added.name} onsaved={() => toolsDone(true)} oncancel={() => toolsDone(false)} />
  {:else}
    <SegmentedControl bind:value={mode} label="How to add it" {options} />
    <p class="meta">
      {#if mode === "preset"}Well-known scanners that run on this computer without an LLM or API key. You install the tool; holdmap adds its manifest, off until you review and approve its permissions.
      {:else if mode === "command"}Any MCP server that speaks stdio. holdmap starts it in the sandbox with a cleared environment, only for runs you start, and stops it afterwards.
      {:else if mode === "url"}An MCP server you run that speaks Streamable HTTP. holdmap connects only for runs you start and doesn't sandbox it.
      {:else if mode === "agents"}One of your agents' configured MCP servers. holdmap starts its own copy: it gets none of your agent's settings or keys.
      {:else}A manifest for a local command-line or MCP scanner (TOML).{/if}
      {demo ? " Browser demo: it's checked and listed, but nothing is installed, started or contacted." : ""}
    </p>
    {#if mode !== "manifest" && mode !== "preset"}
      <div class="grid">
        <label>Name<input bind:value={name} placeholder="My scanner" oninput={nameInput} /></label>
        <label>Id<input bind:value={id} placeholder="local.my-scanner" spellcheck="false" oninput={() => (idEdited = true)} onblur={touch("id")} aria-invalid={!!shown("id")} aria-describedby={shown("id") ? "add-err-id" : undefined} /></label>
        {#if shown("id")}<span class="err" id="add-err-id">{shown("id")}</span>{/if}
      </div>
    {/if}
    {#if mode === "preset"}
      {#if presets === null}<p class="meta">Loading presets…</p>
      {:else}
        <ul class="presets" aria-label="Analyzer presets">
          {#each presets as p (p.id)}
            <li>
              <div class="ph"><span class="nm">{p.name}</span><span class="meta">{p.publisher} · {p.license} · {p.network.length ? `contacts ${p.network.join(", ")}` : "no network"}</span><span class="found" class:yes={p.installed}>{p.installed ? "Installed" : "Not installed"}</span></div>
              <p class="meta">{p.summary}</p>
              {#if !p.installed && p.install}<p class="meta">Install: <code>{p.install}</code></p>{/if}
              <div class="acts">
                {#if p.added}<span class="meta">Added</span>
                {:else}<Button size="sm" variant="secondary" loading={busy} onclick={() => addPreset(p)} aria-label="Add {p.name}">Add</Button>{/if}
                {#if p.homepage}<a class="meta" href={p.homepage} target="_blank" rel="noopener noreferrer">Project page<span class="sr-only"> for {p.name} (opens in a new tab)</span></a>{/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if mode === "command"}
      <label class="wide">Command<input bind:value={command} placeholder="/usr/local/bin/my-mcp-scanner" spellcheck="false" onblur={touch("command")} aria-invalid={!!shown("command")} aria-describedby={shown("command") ? "add-err-command" : undefined} /></label>
        {#if shown("command")}<span class="err" id="add-err-command">{shown("command")}</span>{/if}
      <label class="wide">Arguments (one per line)<textarea rows="2" bind:value={args} spellcheck="false" placeholder="--stdio"></textarea></label>
      <label class="wide">API key names it may get from your keychain (optional)<input bind:value={keys} placeholder="SCANNER_TOKEN" spellcheck="false" onblur={touch("keys")} aria-invalid={!!shown("keys")} aria-describedby={shown("keys") ? "add-err-keys" : undefined} /></label>
        {#if shown("keys")}<span class="err" id="add-err-keys">{shown("keys")}</span>{/if}
    {:else if mode === "url"}
      <label class="wide">URL<input bind:value={url} spellcheck="false" onblur={touch("url")} aria-invalid={!!shown("url")} aria-describedby={shown("url") ? "add-err-url" : undefined} /></label>
        {#if shown("url")}<span class="err" id="add-err-url">{shown("url")}</span>{/if}
      <label class="wide">Bearer token key name (optional; you store the token in the keychain next)<input bind:value={auth} placeholder="SCANNER_TOKEN" spellcheck="false" /></label>
      {#if remote}<Callout tone="warn" size="sm">This server is on another machine. It must use https, gets only skill names, file names and agent ids, and stays off until you explicitly allow it when you turn it on.</Callout>{/if}
    {:else if mode === "agents"}
      {#if servers === null}<p class="meta">Reading your agents' configuration…</p>
      {:else if !servers.length}<p class="meta">No configured MCP servers were found.</p>
      {:else}
        <div class="servers" role="radiogroup" aria-label="Configured MCP servers">
          {#each servers as s, i (s.id + s.agent)}
            <div><label><input type="radio" name="server" checked={picked === s.id} disabled={!s.draft} aria-describedby={s.note ? `server-note-${i}` : undefined} onchange={() => pick(s)} /><span class="nm">{s.name}</span><span class="meta">{s.agent} · {s.transport}{s.draft ? "" : " · can't be added"}</span></label>{#if s.note}<p class="meta note" id={`server-note-${i}`}>{s.note}</p>{/if}</div>
          {/each}
        </div>
        {#if server?.draft}<Callout tone="warn" size="sm">Warning: {server.name} is also {server.agent}'s server. As an analyzer holdmap starts a separate copy for each run you start; anything it does when it starts (signing in, syncing) happens again.</Callout>{/if}
      {/if}
    {:else}
      <label for="analyzer-manifest" class="wide">Analyzer manifest (TOML)</label>
      <textarea id="analyzer-manifest" rows="8" spellcheck="false" bind:value={text} placeholder={'schema = 1\nid = "example.scanner"\n…'} onblur={touch("manifest")} aria-invalid={!!shown("manifest")} aria-describedby={shown("manifest") ? "add-err-manifest" : undefined}></textarea>
      {#if shown("manifest")}<span class="err" id="add-err-manifest">{shown("manifest")}</span>{/if}
    {/if}
    <div class="acts">
      {#if mode !== "preset"}<Button size="sm" variant="primary" loading={busy} onclick={add}>{mode === "manifest" ? "Add analyzer" : "Add and choose tools…"}</Button>{/if}
      <Button size="sm" variant="ghost" onclick={() => { open = startOpen; error = null; touched = {}; oncancel?.(); }}>Cancel</Button>
    </div>
  {/if}
  {/if}
  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
</div>

<style>
  .add { display: grid; gap: var(--sp-2); margin-top: var(--sp-3); }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .head { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 6px 12px; }
  .err { color: var(--danger); font-size: var(--fs-caption); font-weight: var(--fw-regular); }
  input[aria-invalid="true"], textarea[aria-invalid="true"] { border-color: var(--danger); }
  .review { display: grid; gap: 8px; padding: 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--ok-soft); }
  .ok { margin: 0; font-size: var(--fs-body-sm); }
  .facts { margin: 0; padding-left: 18px; color: var(--text-2); font-size: var(--fs-caption); display: grid; gap: 2px; }
  label { display: grid; gap: 2px; font-size: var(--fs-caption); color: var(--text-2); font-weight: var(--fw-medium); }
  .meta { margin: 0; font-size: var(--fs-caption); color: var(--muted); font-weight: var(--fw-regular); }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--sp-2); }
  input, textarea { font: inherit; font-size: var(--fs-caption); padding: 4px 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--input-bg, var(--surface)); color: var(--text); }
  textarea { font-family: var(--mono); resize: vertical; padding: var(--sp-2); }
  .servers { display: grid; gap: 4px; }
  .servers label { display: flex; align-items: center; gap: 6px; }
  .nm { color: var(--text); font-size: var(--fs-body-sm); }
  .note { padding-left: 22px; }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2); }
  .presets { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--sp-2); }
  .presets li { display: grid; gap: 2px; padding: var(--sp-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .ph { display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px; }
  .found { font-size: var(--fs-caption); color: var(--muted); border: 1px solid var(--border); border-radius: var(--r-full); padding: 0 6px; }
  .found.yes { color: var(--ok-text, var(--ok)); border-color: var(--ok); }
  code { font-family: var(--mono); font-size: var(--fs-caption); overflow-wrap: anywhere; }
</style>
