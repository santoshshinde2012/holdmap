<script lang="ts">
  // Configured MCP servers with how each launches and whether it runs now (observed). Shared by
  // the agent card and the overview. Long package names wrap; nothing is cut off.
  import { isUnpinned, launchLine, runState } from "../../lib/agent-config";
  import { tildify } from "../../lib/format";
  import type { AgentMcpServer } from "../../lib/types";

  let { servers, compact = false }: { servers: AgentMcpServer[]; compact?: boolean } = $props();
</script>

<ul class="servers" class:compact>
  {#each servers as s (s.id)}
    {@const run = runState(s)}
    <li>
      <div class="line">
        <span class="name">{s.name}</span>
        <span class="tag">{s.transport === "unknown" ? "?" : s.transport}</span>
        <span class="what mono" data-full>{launchLine(s)}</span>
        {#if s.launch?.pinned === true}<span class="pin ok" title="An exact version or digest is named">pinned</span>{:else if isUnpinned(s)}<span class="pin warn" title="Downloads the latest version at launch">unpinned</span>{/if}
        <span class="run tone-{run.tone}" title={run.detail}><span class="dot" aria-hidden="true"></span>{run.label}{run.tone === "ok" ? ` · ${s.runtime?.pids.join(", ")}` : ""}</span>
      </div>
      <div class="meta">
        <span>{s.scope}</span>
        <span class="mono path">{tildify(s.source_path)}</span>
        {#if s.env_names?.length}<span>env {s.env_names.join(", ")}</span>{/if}
        {#if s.header_names?.length}<span>headers {s.header_names.join(", ")}</span>{/if}
      </div>
    </li>
  {/each}
</ul>

<style>
  .servers { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .compact { grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 8px 20px; }
  li { display: grid; gap: 2px; min-width: 0; }
  .line { display: flex; align-items: baseline; gap: 6px; flex-wrap: wrap; min-width: 0; }
  .name { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); }
  .tag, .pin { font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); }
  .what { min-width: 0; overflow-wrap: anywhere; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .pin.ok { background: var(--ok-soft); color: var(--ok); }
  .pin.warn { background: var(--warn-soft); color: var(--warn); }
  .run { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); white-space: nowrap; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--faint); }
  .tone-ok { color: var(--ok); } .tone-ok .dot { background: var(--ok); }
  .tone-idle .dot { background: transparent; box-shadow: inset 0 0 0 1px var(--faint); }
  .tone-off { text-decoration: line-through; }
  .meta { display: flex; flex-wrap: wrap; gap: 2px 10px; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); min-width: 0; }
  .path { overflow-wrap: anywhere; }
</style>
