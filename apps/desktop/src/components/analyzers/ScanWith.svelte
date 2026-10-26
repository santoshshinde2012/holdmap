<script lang="ts">
  // "Scan with…" for one skill or MCP server: the analyzers that can look at it, their state, and
  // a run button for the ones you already approved. Turning one on stays in Settings, behind its
  // permissions sheet, so consent and sandboxing are unchanged; nothing here runs by itself.
  import Button from "../ui/Button.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { securityRuns } from "../../lib/security-runs.svelte";
  import { stepLabel } from "../../lib/security-view";
  import { installDialog } from "../../lib/install-dialog.svelte";
  import { STATE_LABEL, countsText, scannersFor, subjectCounts, total } from "../../lib/analyzers";

  let { kind, subject, onsettings, onsecurity }: {
    kind: "skill" | "mcp";
    /** The skill's id, to show its own findings (MCP findings are about the packages it launches). */
    subject: string | null;
    onsettings?: () => void;
    onsecurity?: () => void;
  } = $props();
  const scanners = $derived(scannersFor(insightsStore.analyzers, kind));
  const counts = $derived(subject ? subjectCounts(insightsStore.insights, subject) : null);
  let busy = $state<string | null>(null);
  let note = $state<string | null>(null);

  async function scan(id: string, name: string) {
    if (busy) return;
    busy = id; note = null;
    try {
      const a = insightsStore.analyzers.find((x) => x.id === id);
      if (!a) return;
      await securityRuns.start([a]); // the run dock shows progress and refreshes findings
      const s = securityRuns.stepFor(id);
      note = `${name}: ${s?.phase === "done" ? (s.subjects === 0 ? stepLabel(s) : "done") : s?.note ?? s?.phase ?? "done"}`;
    } catch (e) { note = `${name} failed: ${String(e)}`; }
    finally { busy = null; }
  }
</script>

<section class="scan" aria-label="Scan with analyzers">
  <h4>Scan with…</h4>
  {#if counts && total(counts)}<p class="found">Open findings: {countsText(counts, 3)}{#if onsecurity} · <button type="button" class="link" onclick={onsecurity}>Open Security</button>{/if}</p>{/if}
  {#if !scanners.length}
    <p class="muted">No analyzer for {kind === "skill" ? "skills" : "MCP packages"} yet.{#if onsettings} <button type="button" class="link" onclick={onsettings}>Add one</button>{/if}</p>
  {:else}
    <ul>
      {#each scanners as a (a.id)}
        <li>
          <span class="name">{a.name}<small>{STATE_LABEL[a.state]}</small></span>
          {#if a.state === "approved"}
            <Button size="xs" icon="shield" disabled={!!busy} onclick={() => scan(a.id, a.name)}>{busy === a.id ? "Scanning…" : "Scan"}</Button>
          {:else if onsettings}
            {#if a.state === "not_installed"}<Button size="xs" variant="ghost" icon="download" onclick={() => installDialog.show(a.id)}>Install…</Button>{:else}<Button size="xs" variant="ghost" onclick={onsettings}>Review…</Button>{/if}
          {/if}
        </li>
      {/each}
    </ul>
    <p class="muted">Off until you approve it. Scans run sandboxed over every {kind === "skill" ? "skill" : "pinned package"} it covers.</p>
  {/if}
  {#if note}<p class="muted" role="status">{note}</p>{/if}
</section>

<style>
  .scan { display: grid; gap: 8px; margin-top: 18px; padding-top: 14px; border-top: 1px solid var(--border); }
  h4, p { margin: 0; } h4 { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  li { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .name { display: grid; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); }
  .name small, .muted { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .found { color: var(--danger); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .link { border: 0; background: none; padding: 0; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
</style>
