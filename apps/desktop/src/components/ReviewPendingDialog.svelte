<script lang="ts">
  // Shown while a stop plan is being prepared, so a slow review can still be cancelled.
  import { Button, Dialog } from "./ui";

  let { subject, oncancel }: { subject: string; oncancel: () => void } = $props();
</script>

<Dialog title="Reviewing {subject}…" size="sm" onclose={oncancel} initialFocus="[data-primary]">
  <p role="status" aria-busy="true">Checking the current owners and the steps this action would take.</p>
  {#snippet footer()}<Button onclick={oncancel} data-primary>Cancel</Button>{/snippet}
</Dialog>
