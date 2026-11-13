<script lang="ts">
  // One agent's configuration, read from its own files (opt-in, metadata only): MCP servers with
  // how they launch and whether they run now, skills, instruction files, permission rules and
  // safety settings. "Configured" comes from files; "Running" is observed in the process table.
  import Icon from "../Icon.svelte";
  import McpServerList from "./McpServerList.svelte";
  import { DECISION_LABEL, riskySettings } from "../../lib/agent-config";
  import { placesSummary } from "../../lib/agent-memory";
  import { humanBytes, tildify } from "../../lib/format";
  import ExportActions from "../ui/ExportActions.svelte";
  import type { Agent } from "../../lib/types";

  let { agent, oninspect }: { agent: Agent; oninspect?: () => void } = $props();

  const c = $derived(agent.capabilities);
  const servers = $derived(c?.mcp_servers ?? []);
  const instructions = $derived(c?.instructions ?? []);
  const rules = $derived(c?.tool_rules ?? []);
  const settings = $derived(c?.settings ?? []);
  const risky = $derived(new Set(riskySettings(agent).map((r) => r.setting.key)));
  const builtins = $derived(c?.builtin_tools ?? []);
  const places = $derived(c?.locations ?? []);
  const decisions = ["allow", "ask", "deny"] as const;
</script>

<section class="config" aria-label="{agent.name} configuration">
  {#if !c?.inspected}
    <p class="off">
      <Icon name="plug" size={13} />
      <span>MCP servers, skills and tool permissions aren't read yet.</span>
      {#if oninspect}<button type="button" class="link" onclick={oninspect}>Read configuration</button>{/if}
    </p>
  {:else}
    <h3>MCP servers <span class="n">{servers.length + c.more_mcp_servers}</span></h3>
    {#if servers.length}
      <McpServerList {servers} />
    {:else}
      <p class="none">None configured in the inspected locations.</p>
    {/if}

    {#if c.skills.length || instructions.length}
      <h3>Skills &amp; instructions <span class="n">{c.skills.length + c.more_skills + instructions.length}</span></h3>
      <ul class="files">
        {#each c.skills as k (k.id)}
          <li><span class="kind">skill</span><span class="name">{k.declared_name ?? k.name}</span>{#if k.description}<span class="desc">{k.description}</span>{/if}</li>
        {/each}
        {#each instructions as f (f.id)}
          <li><span class="kind">{f.kind}</span><span class="name">{f.name}</span>{#if f.description}<span class="desc">{f.description}</span>{:else}<span class="desc mono">{tildify(f.source_path)}</span>{/if}</li>
        {/each}
      </ul>
    {/if}

    <h3>Tools &amp; permissions</h3>
    {#if settings.length}
      <ul class="settings">
        {#each settings as st (st.key + st.source_path)}
          <li class:risky={risky.has(st.key)}><span class="mono">{st.key}</span><span class="val">{st.value}</span><span class="src mono">{tildify(st.source_path)}</span></li>
        {/each}
      </ul>
    {/if}
    {#each decisions as d}
      {@const list = rules.filter((r) => r.decision === d)}
      {#if list.length}
        <div class="rules"><span class="label d-{d}">{DECISION_LABEL[d]}</span><span class="chips">{#each list as r}<code title={tildify(r.source_path)}>{r.rule}</code>{/each}</span></div>
      {/if}
    {/each}
    {#if !rules.length && !settings.length}<p class="none">No permission rules or safety settings in the inspected files.</p>{/if}
    {#if places.length}
      <h3>Where this agent keeps them <span class="n">{places.length}</span></h3>
      <p class="none">A present file is not proof it was loaded. Lower rank is the more specific documented layer.</p>
      <ul class="files">
        {#each places as place (place.id)}
          <li>
            <span class="kind">{place.scope}</span>
            <span class="name">{place.kind.replaceAll("_", " ")}</span>
            <span class="desc">{place.status}{place.path ? ` · ${tildify(place.path)}` : ""}{place.symlink_target ? ` → ${tildify(place.symlink_target)}` : ""}</span>
            <span class="desc">{place.file_count == null ? "count unknown" : `${place.file_count} entries`} · {place.byte_size == null ? "size unknown" : humanBytes(place.byte_size)}</span>
            {#if place.status_reason}<span class="desc">{place.status_reason}</span>{/if}
            {#if place.candidates.length}<span class="desc">{place.candidates.slice(0, 8).join(", ")}</span>{/if}
          </li>
        {/each}
      </ul>
      <ExportActions label="Export locations" copy={() => placesSummary(places)} files={[{ label: "JSON", filename: `${agent.product}-locations.json`, build: () => places }]} />
    {/if}
    {#if builtins.length}
      <p class="builtin"><span class="label">Built-in</span>{builtins.join(" · ")} <em>(documented, not observed)</em></p>
    {/if}
  {/if}
</section>

<style>
  .config { display: grid; gap: 6px; min-width: 0; }
  h3 { margin: 8px 0 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--muted); }
  .n { font-variant-numeric: tabular-nums; color: var(--faint); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .name { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); }
  .kind, .label { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
  .files li { display: flex; gap: 6px; align-items: baseline; flex-wrap: wrap; min-width: 0; }
  .desc { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); min-width: 0; }
  .settings li { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .settings .val { font-weight: var(--fw-semibold); }
  .settings li.risky .val { color: var(--danger); }
  .src { color: var(--muted); }
  .src, .desc.mono { overflow-wrap: anywhere; }
  .rules { display: flex; gap: 8px; align-items: baseline; min-width: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; min-width: 0; }
  code { font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 5px; border-radius: 5px; background: var(--surface-2); color: var(--text-2); overflow-wrap: anywhere; }
  .d-allow { background: var(--ok-soft); color: var(--ok); }
  .d-ask { background: var(--warn-soft); color: var(--warn); }
  .d-deny { background: var(--danger-soft); color: var(--danger); }
  .builtin { margin: 0; display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .builtin em { color: var(--muted); font-style: normal; }
  .off, .none { margin: 0; display: flex; align-items: center; gap: 6px; flex-wrap: wrap; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .link { border: 0; padding: 0; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  .link:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
</style>
