<script lang="ts">
  // Add your own analyzer: any MCP server (a command holdmap starts, a URL it connects to, or
  // one of your agents' configured servers), or a manifest file. holdmap validates it and keeps
  // it off. For MCP servers the next step lists the tools and maps their inputs.
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import SegmentedControl from "../ui/SegmentedControl.svelte";
  import McpTools from "./McpTools.svelte";
  import * as api from "../../lib/api";
  import type { ConfiguredServer, McpDraft } from "../../lib/analyzers";

  let { onadded, demo = false }: { onadded?: (id: string) => void; demo?: boolean } = $props();
  type Mode = "command" | "url" | "agents" | "manifest";
  let open = $state(false);
  let mode = $state<Mode>("command");
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

  const remote = $derived.by(() => {
    try { const h = new URL(url).hostname.replace(/^\[|\]$/g, ""); return !(h === "localhost" || h === "::1" || /^127\./.test(h)); } catch { return false; }
  });
  const server = $derived(servers?.find((s) => s.id === picked) ?? null);
  const names = (s: string) => s.split(/[\s,]+/).map((x) => x.trim()).filter(Boolean);

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
    if (busy) return;
    busy = true;
    error = null;
    try {
      if (mode === "manifest") {
        // A manifest already names its tools; edit them later with Tools….
        const view = await api.addAnalyzer(text);
        text = "";
        open = false;
        onadded?.(view.id);
      } else {
        const view = await api.addMcpAnalyzer(draft());
        added = { id: view.id, name: view.name };
      }
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  function done() {
    const a = added;
    added = null;
    open = false;
    if (a) onadded?.(a.id);
  }
  const ready = $derived(mode === "manifest" ? !!text.trim() : !!id.trim() && (mode === "command" ? !!command.trim() : mode === "url" ? !!url.trim() : !!server?.draft));
</script>

<div class="add" data-testid="add-analyzer">
  {#if !open}
    <Button size="sm" variant="ghost" icon="plus" onclick={() => (open = true)}>Add your own analyzer…</Button>
  {:else if added}
    <h4>Choose {added.name}'s tools</h4>
    <McpTools id={added.id} name={added.name} onsaved={done} oncancel={done} />
  {:else}
    <h4>Add your own analyzer</h4>
    <SegmentedControl bind:value={mode} label="How to add it" options={[
      { value: "command", label: "Command" }, { value: "url", label: "Local URL" },
      { value: "agents", label: "From your agents" }, { value: "manifest", label: "Manifest" },
    ]} />
    <p class="meta">
      {#if mode === "command"}Any MCP server that speaks stdio. holdmap starts it in the sandbox with a cleared environment, only for runs you start, and stops it afterwards.
      {:else if mode === "url"}An MCP server you run that speaks Streamable HTTP. holdmap connects only for runs you start and doesn't sandbox it.
      {:else if mode === "agents"}One of your agents' configured MCP servers. holdmap starts its own copy: it gets none of your agent's settings or keys.
      {:else}A manifest for a local command-line or MCP scanner (TOML).{/if}
      {demo ? " Browser demo: it's checked and listed, but nothing is installed, started or contacted." : ""}
    </p>
    {#if mode !== "manifest"}
      <div class="grid">
        <label>Id<input bind:value={id} placeholder="local.my-scanner" spellcheck="false" /></label>
        <label>Name<input bind:value={name} placeholder="My scanner" /></label>
      </div>
    {/if}
    {#if mode === "command"}
      <label class="wide">Command<input bind:value={command} placeholder="/usr/local/bin/my-mcp-scanner" spellcheck="false" /></label>
      <label class="wide">Arguments (one per line)<textarea rows="2" bind:value={args} spellcheck="false" placeholder="--stdio"></textarea></label>
      <label class="wide">API key names it may get from your keychain (optional)<input bind:value={keys} placeholder="SCANNER_TOKEN" spellcheck="false" /></label>
    {:else if mode === "url"}
      <label class="wide">URL<input bind:value={url} spellcheck="false" /></label>
      <label class="wide">Bearer token key name (optional; you store the token in the keychain next)<input bind:value={auth} placeholder="SCANNER_TOKEN" spellcheck="false" /></label>
      {#if remote}<Callout tone="warn" size="sm">This server is on another machine. It must use https, gets only skill names, file names and agent ids, and stays off until you explicitly allow it when you turn it on.</Callout>{/if}
    {:else if mode === "agents"}
      {#if servers === null}<p class="meta">Reading your agents' configuration…</p>
      {:else if !servers.length}<p class="meta">No configured MCP servers were found.</p>
      {:else}
        <ul class="servers" role="radiogroup" aria-label="Configured MCP servers">
          {#each servers as s (s.id + s.agent)}
            <li><label><input type="radio" name="server" checked={picked === s.id} disabled={!s.draft} onchange={() => pick(s)} /><span class="nm">{s.name}</span><span class="meta">{s.agent} · {s.transport}{s.draft ? "" : " · can't be added"}</span></label>{#if s.note}<p class="meta note">{s.note}</p>{/if}</li>
          {/each}
        </ul>
        {#if server?.draft}<Callout tone="warn" size="sm">Warning: {server.name} is also {server.agent}'s server. As an analyzer holdmap starts a separate copy for each run you start; anything it does when it starts (signing in, syncing) happens again.</Callout>{/if}
      {/if}
    {:else}
      <label for="analyzer-manifest" class="wide">Analyzer manifest (TOML)</label>
      <textarea id="analyzer-manifest" rows="8" spellcheck="false" bind:value={text} placeholder={'schema = 1\nid = "example.scanner"\n…'}></textarea>
    {/if}
    <div class="acts">
      <Button size="sm" variant="primary" loading={busy} disabled={!ready} onclick={add}>{mode === "manifest" ? "Add analyzer" : "Add and choose tools…"}</Button>
      <Button size="sm" variant="ghost" onclick={() => { open = false; error = null; }}>Cancel</Button>
    </div>
  {/if}
  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
</div>

<style>
  .add { display: grid; gap: var(--sp-2); margin-top: var(--sp-3); }
  h4 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  label { display: grid; gap: 2px; font-size: var(--fs-caption); color: var(--text-2); font-weight: var(--fw-medium); }
  .meta { margin: 0; font-size: var(--fs-caption); color: var(--muted); font-weight: var(--fw-regular); }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--sp-2); }
  input, textarea { font: inherit; font-size: var(--fs-caption); padding: 4px 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--input-bg, var(--surface)); color: var(--text); }
  textarea { font-family: var(--mono); resize: vertical; padding: var(--sp-2); }
  .servers { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .servers label { display: flex; align-items: center; gap: 6px; }
  .nm { color: var(--text); font-size: var(--fs-body-sm); }
  .note { padding-left: 22px; }
  .acts { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
</style>
