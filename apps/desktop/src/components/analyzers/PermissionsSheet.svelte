<script lang="ts">
  // The consent step: exactly what an analyzer gets, built from the real input, with the
  // sandbox stated honestly. Nothing is approved until "Turn on" is pressed.
  import Button from "../ui/Button.svelte";
  import Callout from "../ui/Callout.svelte";
  import type { Preview } from "../../lib/analyzers";

  let { preview, busy = false, error = null, onapprove, oncancel }: {
    preview: Preview;
    busy?: boolean;
    error?: string | null;
    onapprove: () => void;
    oncancel: () => void;
  } = $props();
  const a = $derived(preview.analyzer);
  const changed = $derived(a.state === "changed");
</script>

<section class="sheet" aria-labelledby="sheet-title">
  <header>
    <h3 id="sheet-title">{changed ? "Review again:" : "Turn on"} {a.name}?</h3>
    <p class="by">{a.publisher} · {a.license} · <a href={a.homepage} target="_blank" rel="noreferrer">{a.homepage.replace(/^https:\/\//, "")}</a></p>
    <p>{a.summary}</p>
  </header>
  {#if changed}
    <Callout tone="warn" size="sm" title="Changed since you approved it">{a.changes.join("; ")}</Callout>
  {/if}

  <dl>
    <div>
      <dt>It gets</dt>
      <dd>
        {#if preview.subjects.length}
          <ul class="items">
            {#each preview.subjects as s (s.subject.id)}<li><span class="nm">{s.subject.name}</span> <code>{s.input}</code></li>{/each}
          </ul>
        {:else}<span class="muted">Nothing right now: no matching items were found.</span>{/if}
        {#each preview.skipped as s}<p class="muted">Skipped: {s}</p>{/each}
      </dd>
    </div>
    {#if preview.command}
      <div><dt>Command</dt><dd><code class="block">{preview.command.join(" ")}</code>{#if a.binary}<p class="muted">Program: <code>{a.binary}</code></p>{/if}</dd></div>
    {/if}
    <div>
      <dt>Network</dt>
      <dd>
        {#if a.permissions.network.length}
          <span>{a.permissions.network.join(", ")} <span class="muted">(allowed, not limited to these hosts)</span></span>
          {#if a.permissions.network_reason}<p class="muted">{a.permissions.network_reason}</p>{/if}
          {#each a.permissions.sends as s}<p>Sends: {s}</p>{/each}
        {:else}<span>None declared</span>{/if}
      </dd>
    </div>
    {#each preview.payload as body, i}
      <div><dt>{preview.payload.length > 1 ? `Exact request ${i + 1}` : "Exact request"}</dt><dd><pre aria-label="Exact request body">{body}</pre></dd></div>
    {/each}
    {#if preview.env.length}
      <div><dt>Environment</dt><dd>{preview.env.join(", ")} <span class="muted">(set by holdmap; none of your variables or secrets are passed)</span></dd></div>
    {/if}
    <div><dt>Sandbox</dt><dd>{preview.sandbox}</dd></div>
    <div><dt>When</dt><dd>{preview.schedule}</dd></div>
    <div><dt>Never</dt><dd>holdmap never starts MCP servers for analyzers and keeps only short findings, never matched text.</dd></div>
  </dl>

  {#if error}<Callout tone="danger" size="sm">{error}</Callout>{/if}
  <footer>
    <Button variant="ghost" onclick={oncancel} disabled={busy}>Cancel</Button>
    <Button variant="primary" icon="shield" loading={busy} onclick={onapprove}>Turn on {a.name}</Button>
  </footer>
</section>

<style>
  .sheet { display: grid; gap: var(--sp-3); }
  h3 { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); }
  header p { margin: 2px 0 0; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .by { color: var(--muted) !important; font-size: var(--fs-caption) !important; }
  .by a { color: var(--accent-text, var(--accent)); }
  dl { margin: 0; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  dl > div { display: grid; grid-template-columns: 120px minmax(0, 1fr); gap: var(--sp-3); padding: 8px var(--sp-3); border-bottom: 1px solid var(--border); }
  dl > div:last-child { border-bottom: 0; }
  dt { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); }
  dd { margin: 0; min-width: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); overflow-wrap: anywhere; }
  dd p { margin: 2px 0 0; }
  .muted { color: var(--muted); }
  .items { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .nm { font-weight: var(--fw-medium); }
  code { font-family: var(--mono); font-size: var(--fs-caption); color: var(--text-2); }
  code.block { display: block; }
  pre { margin: 0; padding: 6px 8px; border-radius: var(--r-md); background: var(--surface-2, var(--bg)); font-family: var(--mono); font-size: var(--fs-caption); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 160px; overflow: auto; }
  footer { display: flex; justify-content: flex-end; gap: var(--sp-2); }
  @media (max-width: 560px) { dl > div { grid-template-columns: minmax(0, 1fr); gap: 2px; } }
</style>
