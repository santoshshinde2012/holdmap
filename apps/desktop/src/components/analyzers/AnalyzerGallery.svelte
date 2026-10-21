<script lang="ts">
  // The analyzers you can use, in one place: the ones already set up (with their on/off state)
  // and the ready-made presets, each marked installed or not. Turning one on happens in Settings,
  // behind its permissions sheet; this gallery only shows state and links there.
  import { onMount } from "svelte";
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import * as api from "../../lib/api";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { STATE_LABEL } from "../../lib/analyzers";
  import type { PresetView } from "../../lib/analyzer-presets";

  let { onsettings }: { onsettings?: () => void } = $props();
  let presets = $state<PresetView[]>([]);
  onMount(() => { api.analyzerPresets().then((p) => (presets = p), () => (presets = [])); });
  const extra = $derived(presets.filter((p) => !p.added && !insightsStore.analyzers.some((a) => a.id === p.id)));
</script>

<section class="gallery" aria-labelledby="analyzer-gallery-title">
  <div class="ghead"><h3 id="analyzer-gallery-title">Analyzers</h3>{#if onsettings}<Button size="sm" variant="ghost" icon="sliders" onclick={onsettings}>Manage…</Button>{/if}</div>
  <p class="sub">Off by default. Each one runs sandboxed, and only after you approve its permissions.</p>
  <ul>
    {#each insightsStore.analyzers as a (a.id)}
      <li class="card">
        <span class="top"><b>{a.name}</b><span class="state st-{a.state}">{STATE_LABEL[a.state]}</span></span>
        <span class="who">{a.publisher} · {a.kind === "builtin" ? "MCP packages" : "Skills"}</span>
        <span class="sum">{a.summary}</span>
      </li>
    {/each}
    {#each extra as p (p.id)}
      <li class="card preset">
        <span class="top"><b>{p.name}</b><span class="state" class:st-approved={p.installed}>{p.installed ? "Installed" : "Not installed"}</span></span>
        <span class="who">{p.publisher} · Preset</span>
        <span class="sum">{p.summary}</span>
        {#if p.network.length}<span class="net"><Icon name="globe" size={11} />Contacts {p.network.join(", ")}</span>{/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  .gallery { display: grid; align-content: start; gap: var(--sp-2); }
  .ghead { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); }
  h3 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 220px), 1fr)); gap: 8px; }
  .card { display: grid; align-content: start; gap: 4px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); min-width: 0; }
  .preset { border-style: dashed; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 6px; min-width: 0; }
  b { font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .state { flex: none; padding: 1px 6px; border-radius: var(--r-full); border: 1px solid var(--border); color: var(--muted); font-size: var(--fs-caption); }
  .st-approved { color: var(--ok, var(--tone-green)); border-color: currentColor; }
  .st-changed { color: var(--warn); border-color: currentColor; }
  .who, .net { color: var(--muted); font-size: var(--fs-caption); display: flex; align-items: center; gap: 4px; }
  .sum { color: var(--text-2); font-size: var(--fs-caption); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
</style>
