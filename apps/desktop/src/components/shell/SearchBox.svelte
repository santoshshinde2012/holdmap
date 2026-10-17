<script lang="ts">
  // The title bar's search field: one per view, each bound to that view's own query.
  import { TextField } from "../ui";
  import type { View } from "./ViewToolbar.svelte";

  let { view, capabilities, singleKeys, input = $bindable(), agentQuery = $bindable(), graphQuery = $bindable(), portQuery = $bindable() }: {
    view: View;
    /** Agents is showing MCP & skills. */
    capabilities: boolean;
    singleKeys: boolean;
    input?: HTMLInputElement;
    agentQuery: string;
    graphQuery: string;
    portQuery: string;
  } = $props();
  const hint = $derived(singleKeys ? "/" : undefined);
</script>

{#if view === "agents"}
  <TextField bind:input bind:value={agentQuery} type="search" variant="filled" icon="search" label="Search agents" labelHidden
    placeholder={capabilities ? "Search MCP, skills, sources, PIDs…" : "Search agents, tools, folders, PIDs…"} clearable kbdHint={hint}
    spellcheck="false" autocomplete="off" aria-controls={capabilities ? "agent-capabilities" : "agent-list"} />
{:else if view === "graph"}
  <TextField bind:input bind:value={graphQuery} type="search" variant="filled" icon="search" label="Search services" labelHidden
    placeholder="Search services, projects, ports, PIDs…" clearable kbdHint={hint} spellcheck="false" autocomplete="off" aria-controls="service-graph" />
{:else}
  <TextField bind:input bind:value={portQuery} type="search" variant="filled" icon="search" label="Search ports" labelHidden
    placeholder="Search ports, projects, processes…" clearable kbdHint={hint} spellcheck="false" autocomplete="off" aria-controls="port-list" />
{/if}
