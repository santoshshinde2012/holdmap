<script lang="ts">
  // One agent's configuration, read from its own files (opt-in, metadata only): MCP servers with
  // how they launch and whether they run now, skills, instruction files, permission rules and
  // safety settings. "Configured" comes from files; "Running" is observed in the process table.
  import Icon from "../Icon.svelte";
  import { DECISION_LABEL, isUnpinned, launchLine, riskySettings, runState } from "../../lib/agent-config";
  import { tildify } from "../../lib/format";
  import type { Agent } from "../../lib/types";

  let { agent, oninspect }: { agent: Agent; oninspect?: () => void } = $props();

  const c = $derived(agent.capabilities);
  const servers = $derived(c?.mcp_servers ?? []);
  const instructions = $derived(c?.instructions ?? []);
  const rules = $derived(c?.tool_rules ?? []);
  const settings = $derived(c?.settings ?? []);
  const risky = $derived(new Set(riskySettings(agent).map((r) => r.setting.key)));
  const builtins = $derived(c?.builtin_tools ?? []);
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
      <ul class="servers">
        {#each servers as s (s.id)}
          {@const run = runState(s)}
          <li>
            <div class="line">
              <span class="name">{s.name}</span>
              <span class="tag">{s.transport === "unknown" ? "?" : s.transport}</span>
              <span class="what mono" title={launchLine(s)}>{launchLine(s)}</span>
              {#if s.launch?.pinned === true}<span class="pin ok" title="An exact version or digest is named">pinned</span>{:else if isUnpinned(s)}<span class="pin warn" title="Downloads the latest version at launch">unpinned</span>{/if}
              <span class="run tone-{run.tone}" title={run.detail}><span class="dot" aria-hidden="true"></span>{run.label}{run.tone === "ok" ? ` · ${s.runtime?.pids.join(", ")}` : ""}</span>
            </div>
            <div class="meta">
              <span>{s.scope}</span>
              <span class="mono" title={s.source_path}>{tildify(s.source_path)}</span>
              {#if s.env_names?.length}<span>env {s.env_names.join(", ")}</span>{/if}
              {#if s.header_names?.length}<span>headers {s.header_names.join(", ")}</span>{/if}
            </div>
          </li>
        {/each}
      </ul>
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
    {#if builtins.length}
      <p class="builtin"><span class="label">Built-in</span>{builtins.join(" · ")} <em>(documented, not observed)</em></p>
    {/if}
  {/if}
</section>

<style>
  .config { display: grid; gap: 6px; min-width: 0; }
  h3 { margin: 8px 0 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); }
  .n { font-variant-numeric: tabular-nums; color: var(--faint); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .servers li { display: grid; gap: 2px; min-width: 0; }
  .line { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; min-width: 0; }
  .name { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); }
  .tag, .pin, .kind, .label { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
  .what { flex: 1 1 120px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); font-size: var(--fs-caption); }
  .pin.ok { background: var(--ok-soft); color: var(--ok); }
  .pin.warn { background: var(--warn-soft); color: var(--warn); }
  .run { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); white-space: nowrap; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--faint); }
  .tone-ok { color: var(--ok); } .tone-ok .dot { background: var(--ok); }
  .tone-idle .dot { background: transparent; box-shadow: inset 0 0 0 1px var(--faint); }
  .tone-off { text-decoration: line-through; }
  .meta { display: flex; flex-wrap: wrap; gap: 2px 10px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); min-width: 0; }
  .meta .mono, .src, .desc.mono { overflow-wrap: anywhere; }
  .files li { display: flex; gap: 6px; align-items: baseline; flex-wrap: wrap; min-width: 0; }
  .desc { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); min-width: 0; }
  .settings li { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .settings .val { font-weight: var(--fw-semibold); }
  .settings li.risky .val { color: var(--danger); }
  .src { color: var(--muted); }
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
