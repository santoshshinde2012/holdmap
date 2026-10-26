<script lang="ts">
  // "What a skill analyzer does": why skills matter for security and what holdmap does (and
  // doesn't) do when it runs one. Collapsible; remembers being closed for the session.
  import Icon from "../Icon.svelte";
  let open = $state(sessionStorage.getItem("holdmap.explainer") !== "closed");
  $effect(() => { sessionStorage.setItem("holdmap.explainer", open ? "open" : "closed"); });
</script>

<details class="ex" bind:open data-skill-explainer>
  <summary><Icon name="info" size={13} />What a skill analyzer does</summary>
  <div class="body">
    <p>Agent skills are folders of instructions and scripts that your coding agents load and follow. A skill can tell an agent to run commands, read files or send data out, so a risky skill acts with your agent's access.</p>
    <ul>
      <li><b>What runs:</b> only analyzers you approved, when you click Run or Rescan or, if you allowed it, when a skill changes. holdmap never starts or connects to MCP servers on its own.</li>
      <li><b>What it sees:</b> a copy of each skill folder, with a cleared environment and a time limit. holdmap keeps short findings only.</li>
      <li><b>What you get:</b> for each finding, the agents that load the skill, its scope (global, project or local), the folder and the source file, plus next steps.</li>
      <li><b>Unverified:</b> findings are the analyzer's word. holdmap hasn't checked them; review before you act.</li>
    </ul>
  </div>
</details>

<style>
  .ex { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  summary { display: flex; align-items: center; gap: 6px; padding: 8px 12px; cursor: pointer; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  summary:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; border-radius: var(--r-lg); }
  .body { padding: 0 14px 12px; color: var(--text-2); font-size: var(--fs-body-sm); }
  p { margin: 0 0 6px; }
  ul { margin: 0; padding-left: 18px; display: grid; gap: 3px; }
  b { color: var(--text); }
</style>
