<script lang="ts">
  // The last step of adding an analyzer: what was added, that it is off, and the one next step.
  import Button from "../../ui/Button.svelte";
  let { name, id, source, tools, onreview, ondone }:
    { name: string; id: string; source: string; tools: string | null; onreview?: () => void; ondone: () => void } = $props();
</script>

<section class="review" aria-labelledby="rv-{id}">
  <header>
    <span class="logo" aria-hidden="true">{(name || id || "?").charAt(0).toUpperCase()}</span>
    <div class="who"><h4 id="rv-{id}">{name || id}</h4><code>{id}</code></div>
    <span class="pill"><i aria-hidden="true"></i>Added · Off</span>
  </header>
  <p class="lead" role="status">{name || id} is added and off. Nothing runs until you approve it.</p>
  <dl>
    <div><dt>Source</dt><dd>{source}</dd></div>
    {#if tools}<div><dt>Tools</dt><dd>{tools}</dd></div>{/if}
    <div><dt>Runs</dt><dd>Only when you start a scan, sandboxed, on a private copy of each skill</dd></div>
    {#if tools}<div><dt>If tools change</dt><dd>holdmap asks you again before the next run</dd></div>{/if}
  </dl>
  <footer>
    <Button size="sm" variant="ghost" onclick={ondone}>Done</Button>
    {#if onreview}<Button size="sm" variant="primary" icon="shield" onclick={onreview}>Review and turn on…</Button>{/if}
  </footer>
</section>

<style>
  .review { display: grid; gap: 14px; padding: 16px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  header { display: grid; grid-template-columns: 36px minmax(0, 1fr) auto; align-items: center; gap: 12px; }
  .logo { display: grid; place-items: center; width: 36px; height: 36px; border-radius: var(--r-md); background: var(--accent-soft); color: var(--accent); font-weight: var(--fw-semibold); }
  .who { display: grid; min-width: 0; }
  h4 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  code { font-family: var(--mono); font-size: var(--fs-caption); color: var(--muted); }
  .pill { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--r-full); color: var(--text-2); font-size: var(--fs-caption); font-weight: var(--fw-medium); white-space: nowrap; }
  .pill i { width: 7px; height: 7px; border-radius: 50%; background: var(--border-strong); }
  .lead { margin: 0; color: var(--text-2); font-size: var(--fs-body-sm); }
  dl { margin: 0; display: grid; border: 1px solid var(--border); border-radius: var(--r-md); overflow: hidden; }
  dl div { display: grid; grid-template-columns: 140px minmax(0, 1fr); gap: 12px; padding: 8px 12px; font-size: var(--fs-body-sm); }
  dl div + div { border-top: 1px solid var(--border); }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text); overflow-wrap: anywhere; }
  footer { display: flex; justify-content: flex-end; gap: 8px; }
  @media (max-width: 520px) { dl div { grid-template-columns: minmax(0, 1fr); gap: 2px; } }
</style>
