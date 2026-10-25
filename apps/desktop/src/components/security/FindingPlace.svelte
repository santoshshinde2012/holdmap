<script lang="ts">
  // Where a finding lives and what to do about it: the agents that load the skill, its scope,
  // the folder holdmap scanned and the file the analyzer pointed at. Says "unknown" when
  // holdmap can't confirm a fact; findings are the analyzer's word, unverified.
  import Button from "../ui/Button.svelte";
  import CopyValue from "../ui/CopyValue.svelte";
  import Icon from "../Icon.svelte";
  import * as api from "../../lib/api";
  import { redactHome } from "../../lib/report-export";
  import { scopeLabel, suggestedActions, type FindingItem } from "../../lib/security-view";

  let { item }: { item: FindingItem } = $props();
  let note = $state<string | null>(null);
  const actions = $derived(suggestedActions(item));
  const copy = (t: string) => navigator.clipboard?.writeText(t);

  async function reveal() {
    note = null;
    try {
      const r = await api.revealAnalyzerSubject(item.analyzer, item.finding.subject);
      if (!r.revealed) {
        if (r.folder) await copy(r.folder);
        note = r.folder ? "The browser can't open folders, so the folder path was copied." : "holdmap doesn't know this folder.";
      }
    } catch (e) { note = e instanceof Error ? e.message : String(e); }
  }
</script>

<div class="place" data-finding-place>
  <dl>
    <div><dt>Agents</dt><dd>{item.agents.length ? item.agents.join(", ") : "unknown"}</dd></div>
    <div><dt>Skill</dt><dd>{item.subject?.name ?? item.finding.subject}</dd></div>
    <div><dt>Scope</dt><dd>{scopeLabel(item.scope)}</dd></div>
    <div class="wide"><dt>Folder</dt><dd>
      {#if item.folder}
        <span class="row"><CopyValue value={item.folder} display={redactHome(item.folder)} mono what="Folder path" oncopy={copy} />
          <Button size="xs" variant="ghost" icon="folder" onclick={reveal}>{api.isTauri ? "Show in folder" : "Copy folder"}</Button></span>
      {:else}unknown — holdmap couldn't confirm where this skill lives{/if}
    </dd></div>
    <div class="wide"><dt>Source path</dt><dd>
      {#if item.path}<CopyValue value={item.path + (item.finding.location.line ? `:${item.finding.location.line}` : "")} display={redactHome(item.path) + (item.finding.location.line ? `:${item.finding.location.line}` : "")} mono what="Source path" oncopy={copy} />{:else}unknown{/if}
    </dd></div>
  </dl>
  {#if note}<p class="note" role="status">{note}</p>{/if}
  <div class="todo">
    <h5><Icon name="sparkles" size={12} />Suggested next steps</h5>
    <ol>{#each actions as a (a.kind)}<li>{a.label}</li>{/each}</ol>
    <p class="unv">Reported by {item.analyzerName}; holdmap hasn't verified it.</p>
  </div>
</div>

<style>
  .place { display: grid; gap: 10px; }
  dl { margin: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 8px 16px; }
  .wide { grid-column: 1 / -1; }
  dt { color: var(--muted); font-size: var(--fs-caption); }
  dd { margin: 0; font-size: var(--fs-body-sm); color: var(--text); overflow-wrap: anywhere; min-width: 0; }
  .row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .row > :global(:first-child) { min-width: 0; flex: 1; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-caption); }
  .todo { display: grid; gap: 4px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); }
  h5 { display: flex; align-items: center; gap: 6px; margin: 0; font-size: var(--fs-caption); font-weight: var(--fw-semibold); color: var(--text); }
  ol { margin: 0; padding-left: 18px; font-size: var(--fs-body-sm); color: var(--text-2); display: grid; gap: 2px; }
  .unv { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
</style>
