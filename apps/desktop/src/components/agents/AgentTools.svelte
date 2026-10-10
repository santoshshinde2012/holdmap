<script lang="ts">
  import Icon from "../Icon.svelte";
  import { EVIDENCE_LABEL, TOOL_LABEL } from "../../lib/agents";
  import { humanBytes, tildify } from "../../lib/format";
  import type { AgentTool } from "../../lib/types";

  let { tools, more = 0 }: { tools: AgentTool[]; more?: number } = $props();
</script>

{#if tools.length || more}
  <section aria-label="Agent tools">
    <h3>Tools &amp; commands</h3>
    <ul>
      {#each tools as tool (tool.pid)}
        <li>
          <div class="identity">
            <Icon name={tool.kind === "mcp_server" ? "plug" : tool.kind === "dev_server" ? "server" : "terminal"} size={13} />
            <span class="name">{tool.name}</span>
            <span class="pid mono">{tool.pid}</span>
          </div>
          <span class="meta">{TOOL_LABEL[tool.kind]}{#if tool.evidence !== "observed"}{" · "}<span class="evidence">{EVIDENCE_LABEL[tool.evidence]}</span>{/if}{#if tool.ports.length}{" · :"}{tool.ports.join(", :")}{:else if tool.kind === "mcp_server"}{" · no listening port"}{/if}</span>
          {#if tool.cwd}<span class="meta path" title={tool.cwd}>{tildify(tool.cwd)}</span>{/if}
          <span class="meta mono">{tool.cpu_percent.toFixed(tool.cpu_percent < 10 ? 1 : 0)}% CPU · {humanBytes(tool.memory_bytes)}</span>
          {#if tool.command}<details><summary>Command</summary><code>{tool.command}</code></details>{/if}
        </li>
      {/each}
      {#if more}<li class="meta">+{more} tools not listed</li>{/if}
    </ul>
  </section>
{/if}

<style>
  section { display: grid; gap: 6px; }
  h3 { margin: 8px 0 0; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  li { min-width: 0; display: grid; gap: 1px; }
  .identity { display: flex; align-items: center; gap: 6px; color: var(--text-2); }
  .name { min-width: 0; overflow-wrap: anywhere; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); }
  .pid, .meta { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .pid { margin-left: auto; }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .evidence { color: var(--warn); }
  details { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  summary { cursor: pointer; width: fit-content; }
  code { display: block; margin-top: 4px; padding: 6px 8px; border-radius: var(--r-sm); background: var(--surface-2); color: var(--text-2); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow-wrap: anywhere; user-select: text; }
</style>
