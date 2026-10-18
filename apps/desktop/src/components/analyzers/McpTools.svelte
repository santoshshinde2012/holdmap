<script lang="ts">
  // Choose an MCP analyzer's tools and map their inputs from the inventory, guided by each
  // tool's input schema (holdmap suggests a mapping), with the exact arguments previewed; then
  // say where findings sit in the output. Listing the tools starts or contacts the server once,
  // only when the user clicks; no tool is called until the analyzer is approved and run.
  import { onMount } from "svelte";
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import * as api from "../../lib/api";
  import type { OutputMapping, ToolInfo, ToolReport, ToolSpec } from "../../lib/analyzers";
  import { checkArguments, initialSpec, parseValue, placeholdersFor, preselect, previewArguments, schemaFields, valueText } from "../../lib/mcp-mapping";

  let { id, name, onsaved, oncancel }: { id: string; name: string; onsaved?: () => void; oncancel?: () => void } = $props();
  let report = $state<ToolReport | null>(null);
  let listedBefore = $state<boolean | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let chosen = $state<string[]>([]);
  let specs = $state<Record<string, ToolSpec>>({});
  let custom = $state<Record<string, boolean>>({});
  let output = $state<OutputMapping>({});

  const SAMPLE = { dir: "<private copy of example-skill>", skill_md: "<text of SKILL.md>", files: ["SKILL.md", "scripts/run.sh"], name: "example-skill", agents: ["claude-code"] };
  const OUTPUT_FIELDS: [keyof OutputMapping, string, string][] = [
    ["items", "Finding list", "$.issues[*]"], ["rule_id", "Rule id field", "code"], ["severity", "Severity field", "level"],
    ["title", "Title field", "msg"], ["file", "File field", "file"], ["line", "Line field", "line"],
  ];

  function load(r: ToolReport) {
    report = r;
    chosen = preselect(r);
    specs = Object.fromEntries(r.snapshot.tools.map((t) => [t.name, initialSpec(r, t)]));
    custom = {};
    output = { ...r.output };
  }

  onMount(async () => {
    try { load(await api.analyzerTools(id)); listedBefore = true; }
    catch { listedBefore = false; }
  });

  async function list() {
    busy = true;
    error = null;
    try { load(await api.listAnalyzerTools(id)); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  function toggle(t: ToolInfo, on: boolean) {
    chosen = on ? [...chosen, t.name] : chosen.filter((n) => n !== t.name);
  }

  function setArg(tool: string, field: string, raw: string, isCustom: boolean) {
    const args = { ...specs[tool].args };
    if (raw === "" && !isCustom) delete args[field];
    else args[field] = isCustom ? parseValue(raw) : raw;
    specs[tool] = { ...specs[tool], args };
  }

  const selectValue = (tool: string, field: string): string => {
    const v = specs[tool]?.args[field];
    if (custom[`${tool}.${field}`]) return "__custom";
    if (v === undefined) return "";
    return typeof v === "string" && report?.placeholders.some((p) => p.token === v) ? v : "__custom";
  };

  async function save() {
    if (!report) return;
    busy = true;
    error = null;
    try {
      const tools = report.snapshot.tools.filter((t) => chosen.includes(t.name)).map((t) => $state.snapshot(specs[t.name]));
      await api.mapAnalyzerTools(id, tools);
      await api.mapAnalyzerOutput(id, output.items ? $state.snapshot(output) : {});
      onsaved?.();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  const suggestion = $derived(report ? Object.entries(report.output_suggestions).find(([t]) => chosen.includes(t)) ?? null : null);
</script>

<section class="tools" aria-label={`Tools of ${name}`}>
  {#if listedBefore === false && !report}
    <p class="meta">holdmap needs the server's tool list to suggest a mapping. <strong>List tools</strong> starts or contacts the server once, asks for its tools and stops. No tool is called.</p>
    <div class="acts">
      <Button size="sm" variant="primary" icon="refresh" loading={busy} onclick={list}>List tools</Button>
      {#if oncancel}<Button size="sm" variant="ghost" onclick={oncancel}>Close</Button>{/if}
    </div>
  {:else if report}
    {@const s = report.snapshot}
    <header>
      <span class="srv">{s.server?.name ?? "MCP server"}{s.server?.version ? ` ${s.server.version}` : ""}</span>
      <span class="meta">{s.tools.length} {s.tools.length === 1 ? "tool" : "tools"} · pin {s.sha256.slice(0, 12)} · {report.approved ? "approved list" : "not approved yet"}</span>
      <Button size="sm" variant="ghost" icon="refresh" loading={busy} onclick={list}>List again</Button>
    </header>
    {#each report.server_flags as f}<Callout tone="warn" size="sm">Server instructions: {f}</Callout>{/each}
    {#if report.changes.length}<Callout tone="warn" size="sm" title="Changed since you approved it">{report.changes.join("; ")}</Callout>{/if}
    {#if s.truncated}<Callout tone="warn" size="sm">The server lists more tools than holdmap keeps.</Callout>{/if}
    <ul class="list">
      {#each s.tools as t (t.name)}
        {@const on = chosen.includes(t.name)}
        {@const flags = report.flags[t.name] ?? []}
        <li class:flagged={flags.length}>
          <label class="pick">
            <input type="checkbox" checked={on} onchange={(e) => toggle(t, e.currentTarget.checked)} aria-label={t.title ? `${t.title} (${t.name})` : t.name} aria-describedby={t.description ? `desc-${t.name}` : undefined} />
            <span class="tn">{t.title ?? t.name}</span>{#if t.title}<code>{t.name}</code>{/if}
          </label>
          {#if t.description}<p class="desc" id={`desc-${t.name}`}>{t.description.length > 280 ? t.description.slice(0, 280) + "…" : t.description}</p>{/if}
          {#each flags as f}<p class="flag">⚠ {f}</p>{/each}
          {#if on}
            {@const fields = schemaFields(t.input_schema)}
            {@const args = previewArguments(specs[t.name].args, t.input_schema, SAMPLE)}
            {@const problems = checkArguments(args, t.input_schema)}
            {#if fields.length}
              <div class="map" role="group" aria-label={`Inputs of ${t.name}`}>
                {#each fields as f (f.name)}
                  {@const key = `${t.name}.${f.name}`}
                  {@const sel = selectValue(t.name, f.name)}
                  <label for={`in-${key}`} class="fl">{f.name}{f.required ? " *" : ""} <span class="meta">{f.types.join(" | ") || "any"}</span></label>
                  <div class="fv">
                    <select id={`in-${key}`} value={sel} onchange={(e) => { const v = e.currentTarget.value; custom[key] = v === "__custom"; if (v !== "__custom") setArg(t.name, f.name, v, false); }}>
                      <option value="">Not sent</option>
                      {#each placeholdersFor(f, report.placeholders) as p}<option value={p.token}>{p.label}</option>{/each}
                      <option value="__custom">Fixed value…</option>
                    </select>
                    {#if sel === "__custom"}
                      <input aria-label={`Fixed value for ${f.name}`} value={valueText(specs[t.name].args[f.name])} placeholder={f.enumValues ? f.enumValues.map(String).join(" | ") : "text, number, true/false or JSON"} oninput={(e) => setArg(t.name, f.name, e.currentTarget.value, true)} />
                    {/if}
                    {#if f.description}<span class="meta">{f.description}</span>{/if}
                  </div>
                {/each}
              </div>
            {/if}
            <p class="meta">Arguments for one skill (example):</p>
            <pre aria-label={`Example arguments for ${t.name}`}>{JSON.stringify(args, null, 2)}</pre>
            {#each problems as p}<p class="flag">✕ {p}</p>{/each}
          {/if}
        </li>
      {/each}
    </ul>
    <fieldset class="out">
      <legend>Output</legend>
      <p class="meta">SARIF and holdmap JSON are recognised on their own. For other JSON, say where the findings are (JSONPath-style). Anything unmapped is kept as a short excerpt, never as findings.</p>
      {#if suggestion && !output.items}<Button size="sm" variant="ghost" onclick={() => (output = { ...suggestion[1] })}>Use the mapping suggested by {suggestion[0]}</Button>{/if}
      <div class="ofields">
        {#each OUTPUT_FIELDS as [k, label, ph]}
          <label>{label}<input value={output[k] ?? ""} placeholder={ph} oninput={(e) => (output = { ...output, [k]: e.currentTarget.value || null })} /></label>
        {/each}
      </div>
    </fieldset>
    <div class="acts">
      <Button size="sm" variant="primary" loading={busy} disabled={!chosen.length} onclick={save}>Save tools</Button>
      {#if oncancel}<Button size="sm" variant="ghost" onclick={oncancel}>Cancel</Button>{/if}
      <span class="meta">Saving needs your approval again before the next run.</span>
    </div>
  {:else}
    <p class="meta">Loading…</p>
  {/if}
  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
</section>

<style>
  .tools { display: grid; gap: var(--sp-2); padding: var(--sp-2) 0; }
  header { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px var(--sp-2); }
  .srv { font-weight: var(--fw-semibold); font-size: var(--fs-body-sm); }
  .meta { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .list li { display: grid; gap: 4px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); }
  .list li.flagged { border-color: var(--warn); }
  .pick { display: flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); font-weight: var(--fw-medium); }
  code { font-family: var(--mono); font-size: var(--fs-caption); color: var(--muted); }
  .desc { margin: 0; color: var(--text-2); font-size: var(--fs-caption); overflow-wrap: anywhere; }
  .flag { margin: 0; color: var(--warn-text, var(--warn)); font-size: var(--fs-caption); }
  .map { display: grid; grid-template-columns: minmax(110px, 160px) minmax(0, 1fr); gap: 4px var(--sp-2); align-items: start; }
  .fl { font-family: var(--mono); font-size: var(--fs-caption); padding-top: 4px; overflow-wrap: anywhere; }
  .fv { display: grid; gap: 2px; min-width: 0; }
  select, input { font: inherit; font-size: var(--fs-caption); padding: 3px 6px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-sm, 6px); background: var(--input-bg, var(--surface)); color: var(--text); min-width: 0; }
  pre { margin: 0; padding: 6px 8px; border-radius: var(--r-md); background: var(--surface-2, var(--bg)); font-family: var(--mono); font-size: var(--fs-caption); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 140px; overflow: auto; }
  .out { border: 1px solid var(--border); border-radius: var(--r-md); padding: 8px 10px; display: grid; gap: 6px; }
  legend { font-size: var(--fs-caption); font-weight: var(--fw-semibold); padding: 0 4px; }
  .ofields { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 6px; }
  .ofields label { display: grid; gap: 2px; font-size: var(--fs-caption); color: var(--muted); }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2); }
  @media (max-width: 520px) { .map { grid-template-columns: minmax(0, 1fr); } }
</style>
