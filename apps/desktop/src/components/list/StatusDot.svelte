<script lang="ts">
  // Leading status column of a list row: a 7px dot whose colour is the port's state.
  import { tooltip } from "../../lib/tooltip";
  import type { Status } from "../../lib/rows";
  let { status, label }: { status: Status; label: string } = $props();
</script>

<span class="dot {status}" role="img" aria-label={label} use:tooltip={label}></span>

<style>
  .dot { display: block; width: 7px; height: 7px; border-radius: 50%; justify-self: center; background: var(--faint); }
  .listening { background: var(--ok); box-shadow: 0 0 0 2.5px var(--ok-soft); }
  .bound { background: var(--tone-blue); box-shadow: 0 0 0 2.5px var(--tone-blue-bg); }
  .other { background: transparent; box-shadow: inset 0 0 0 1.5px var(--faint); }
  .hidden { background: transparent; box-shadow: inset 0 0 0 1.5px var(--border-strong); }
  .busy { background: var(--warn); box-shadow: 0 0 0 2.5px var(--warn-soft); animation: pulse 0.9s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }
</style>
