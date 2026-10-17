<script lang="ts">
  // One compact statement of how strong a piece of evidence is, with the caveat in a tooltip
  // instead of a repeated paragraph. The caveat is also in the accessible text.
  import { tooltip } from "../../lib/tooltip";
  import type { EvidenceTone } from "../../lib/evidence";

  let { tone, label, detail }: {
    tone: EvidenceTone;
    /** Short visible label, e.g. "Observed" or "Configured, not enforced". */
    label: string;
    /** The caveat: what this evidence does and does not establish. */
    detail: string;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<span class="chip t-{tone}" tabindex="0" use:tooltip={detail}><i aria-hidden="true"></i><span>{label}</span><span class="sr-only">: {detail}</span></span>

<style>
  .chip { display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px; border-radius: var(--r-full); border: 1px solid var(--border); background: var(--surface); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; cursor: help; }
  .chip:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .t-seen i { background: var(--ok); }
  .t-inferred i { background: var(--tone-amber); }
  .t-configured i { background: var(--tone-blue); }
</style>
