<script lang="ts">
  // Pipeline detail: what the analyzer's last run found, per skill — folder, agents and scope.
  import { redactHome } from "../../lib/report-export";
  import { scopeLabel, type FindingItem } from "../../lib/security-view";
  let { analyzer, items }: { analyzer: string; items: FindingItem[] } = $props();
  const rows = $derived.by(() => {
    const m = new Map<string, { name: string; folder: string | null; agents: string[]; scope: string | null; n: number }>();
    for (const i of items) {
      if (i.analyzer !== analyzer) continue;
      const k = i.finding.subject;
      const r = m.get(k) ?? { name: i.subject?.name ?? k, folder: i.folder, agents: i.agents, scope: i.scope, n: 0 };
      r.n++; m.set(k, r);
    }
    return [...m.values()].sort((a, b) => b.n - a.n);
  });
</script>

<section class="ps" aria-labelledby="ps-h" data-pipeline-subjects>
  <h3 id="ps-h">Skills with findings in the last run</h3>
  {#if rows.length}
    <table>
      <thead><tr><th scope="col">Skill</th><th scope="col">Agents</th><th scope="col">Scope</th><th scope="col">Folder</th><th scope="col">Findings</th></tr></thead>
      <tbody>{#each rows as r (r.name + r.folder)}<tr><td>{r.name}</td><td>{r.agents.join(", ") || "unknown"}</td><td>{scopeLabel(r.scope)}</td><td class="dir">{r.folder ? redactHome(r.folder) : "unknown"}</td><td>{r.n}</td></tr>{/each}</tbody>
    </table>
  {:else}<p class="sub">No findings from this analyzer yet.</p>{/if}
</section>

<style>
  .ps { display: grid; gap: 6px; }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .sub { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body-sm); border: 1px solid var(--border); background: var(--surface); }
  th { text-align: left; padding: 6px 10px; background: var(--surface-2); color: var(--text-2); }
  td { padding: 6px 10px; border-top: 1px solid var(--border); }
  .dir { overflow-wrap: anywhere; color: var(--text-2); }
</style>
