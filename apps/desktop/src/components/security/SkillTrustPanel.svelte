<script lang="ts">
  // Skill trust: each skill folder's fingerprint (SHA-256 over names and bytes, bounded and
  // symlink-safe) and whether it changed since you trusted it. What changed is names and sizes
  // only; holdmap never shows or sends file contents.
  import Icon from "../Icon.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyPanel from "../ui/EmptyPanel.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { alerts, bytes, TRUST_LABEL, type ChangeSummary, type SkillTrustView } from "../../lib/trust";
  import { ago } from "../../lib/analyzers";

  let open = $state<string | null>(null);
  let reasons = $state<Record<string, string>>({});
  const report = $derived(trustStore.report);
  $effect(() => { if (!trustStore.report && !trustStore.busy) void trustStore.load(); });

  const when = (ms: number | null | undefined) => (ms ? ago(ms) : "—");
  const shortHash = (h: string | null | undefined) => (h ? `${h.slice(0, 12)}…` : "—");
  function changeLines(c: ChangeSummary): { kind: string; path: string; size: string }[] {
    return [
      ...c.added.map((f) => ({ kind: "Added", path: f.path, size: bytes(f.size_after) })),
      ...c.removed.map((f) => ({ kind: "Removed", path: f.path, size: bytes(f.size_before) })),
      ...c.modified.map((f) => ({ kind: "Changed", path: f.path, size: `${bytes(f.size_before)} → ${bytes(f.size_after)}` })),
    ];
  }
  const hidden = (c: ChangeSummary) => c.added_total + c.removed_total + c.modified_total - c.added.length - c.removed.length - c.modified.length;
  async function trust(s: SkillTrustView, yes: boolean) {
    await trustStore.set(s.subject.id, yes, yes ? reasons[s.subject.id] : undefined);
    if (!trustStore.error) reasons[s.subject.id] = "";
  }
</script>

<section class="trust" aria-labelledby="trust-h">
  <div class="bar">
    <h3 id="trust-h">Skill trust</h3>
    <span class="sub">{#if report?.scanned_ms}Fingerprinted {ago(report.scanned_ms)}{:else}Not fingerprinted yet{/if} · names, sizes and hashes only, never contents</span>
    <Button size="sm" variant="secondary" icon="refresh" loading={trustStore.busy} onclick={() => trustStore.scan()}>Check skills now</Button>
  </div>
  {#if trustStore.error}<p class="err" role="alert"><Icon name="alert" size={12} />{trustStore.error}</p>{/if}
  {#if !report}
    <p class="sub" role="status">Loading…</p>
  {:else if !report.skills.length}
    <EmptyPanel icon="shield" title="No skills fingerprinted yet" hint="Check skills now to record a fingerprint of each skill folder. Nothing leaves this computer." />
  {:else}
    <p class="sum" role="status">{report.changed} changed since trusted · {report.untrusted} not trusted yet · {report.skills.length} skills</p>
    <ul class="list">
      {#each report.skills as s (s.subject.id)}
        {@const id = `trust-${s.subject.id.replace(/[^a-z0-9_-]/gi, "-")}`}
        <li class="row st-{s.state}" class:alert={alerts(s.state)}>
          <button type="button" class="head" aria-expanded={open === s.subject.id} aria-controls={id} onclick={() => (open = open === s.subject.id ? null : s.subject.id)}>
            <Icon name={open === s.subject.id ? "chevron-down" : "chevron"} size={12} />
            <span class="name">{s.subject.name}</span>
            <span class="pill">{TRUST_LABEL[s.state]}</span>
            <span class="meta">{s.subject.agents.join(", ")}{#if s.changed_since_last_scan} · changed since last check{/if}</span>
            <span class="meta">{s.file_count} files · {bytes(s.total_bytes)}</span>
          </button>
          {#if open === s.subject.id}
            <div class="body" {id}>
              <dl>
                <div><dt>Source</dt><dd>{s.subject.scope ? `${s.subject.scope} skill` : "Skill"} · {s.subject.agents.join(", ") || "unknown agent"}{#if s.subject.dir} · <code>{s.subject.dir}</code>{/if}</dd></div>
                <div><dt>Fingerprint</dt><dd><code title={s.sha256 ?? undefined}>{shortHash(s.sha256)}</code>{#if s.partial} <span class="sub">(first files only; large folder)</span>{/if}</dd></div>
                <div><dt>Last modified</dt><dd>{when(s.modified_ms)}</dd></div>
                <div><dt>First seen</dt><dd>{when(s.first_seen_ms)}</dd></div>
                <div><dt>Trusted</dt><dd>{when(s.trusted_ms)}{#if s.reason} · {s.reason}{/if}</dd></div>
              </dl>
              {#if s.error}<p class="err"><Icon name="alert" size={12} />{s.error}</p>{/if}
              {#if s.changes}
                <h4>What changed since you trusted it</h4>
                <ul class="changes" aria-label="Changed files for {s.subject.name}">
                  {#each changeLines(s.changes) as c (c.kind + c.path)}<li><span class="k k-{c.kind.toLowerCase()}">{c.kind}</span><code>{c.path}</code><span class="sub">{c.size}</span></li>{/each}
                </ul>
                {#if hidden(s.changes) > 0}<p class="sub">and {hidden(s.changes)} more</p>{/if}
                <p class="sub">Open the folder and read the changes before trusting this version.</p>
              {/if}
              <div class="acts">
                {#if s.state !== "trusted"}
                  <input class="reason" type="text" maxlength="200" placeholder="Reason (optional)" aria-label="Reason for trusting {s.subject.name}" bind:value={reasons[s.subject.id]} />
                  <Button size="xs" icon="check" loading={trustStore.busy} disabled={s.state === "unreadable"} onclick={() => trust(s, true)}>Trust this version</Button>
                {:else}
                  <Button size="xs" variant="ghost" icon="undo" loading={trustStore.busy} onclick={() => trust(s, false)}>Stop trusting</Button>
                {/if}
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .trust { display: grid; gap: 10px; min-width: 0; }
  .bar { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .bar :global(.ui-btn) { margin-left: auto; }
  h3 { margin: 0; font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  h4 { margin: 4px 0 0; font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .sub, .sum { margin: 0; color: var(--muted); font-size: var(--fs-caption); }
  .err { display: flex; gap: 6px; align-items: center; margin: 0; color: var(--danger); font-size: var(--fs-caption); }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  .row { border-top: 1px solid var(--border); }
  .row:first-child { border-top: 0; }
  .head { display: grid; grid-template-columns: 12px minmax(0, 1fr) auto; grid-template-areas: "c n p" ". m m" ". s s"; align-items: center; gap: 2px 10px; width: 100%; padding: 9px 14px; border: 0; background: none; color: var(--text); text-align: left; font: inherit; cursor: pointer; }
  .head > :global(svg) { grid-area: c; }
  .name { grid-area: n; font-size: var(--fs-body-sm); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pill { grid-area: p; padding: 1px 6px; border-radius: var(--r-sm); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); white-space: nowrap; }
  .st-changed .pill { background: var(--danger-soft); color: var(--danger); }
  .st-new .pill { background: var(--warn-soft, var(--surface-2)); color: var(--warn, var(--text)); }
  .meta { grid-column: 2 / -1; color: var(--muted); font-size: var(--fs-caption); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (min-width: 720px) {
    .head { grid-template-columns: 12px minmax(0, 1fr) auto minmax(0, 1fr) auto; grid-template-areas: none; }
    .head > :global(svg), .name, .pill, .meta { grid-area: auto; grid-column: auto; }
  }
  .head:hover { background: var(--row-hover); }
  .head:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .body { display: grid; gap: 8px; padding: 4px 14px 12px 36px; }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 4px 12px; margin: 0; font-size: var(--fs-caption); }
  dl div { display: contents; }
  dt { color: var(--muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .changes { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; font-size: var(--fs-caption); }
  .changes li { display: flex; gap: 8px; align-items: baseline; min-width: 0; }
  .changes code { overflow-wrap: anywhere; }
  .k { min-width: 60px; font-weight: var(--fw-semibold); }
  .k-added { color: var(--ok, var(--text)); }
  .k-removed, .k-changed { color: var(--danger); }
  .acts { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .reason { flex: 1 1 200px; max-width: 360px; height: 24px; padding: 0 8px; border: 1px solid var(--input-border, var(--border)); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-caption); }
  .reason:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>
