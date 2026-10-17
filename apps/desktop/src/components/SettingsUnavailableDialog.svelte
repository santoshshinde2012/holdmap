<script lang="ts">
  // Settings need the loaded config; until then, say why and offer a retry.
  import { Button, Dialog } from "./ui";

  let { error, loading, onretry, onclose }: { error: string | null; loading: boolean; onretry: () => void; onclose: () => void } = $props();
</script>

<Dialog title="Settings unavailable" size="sm" {onclose} initialFocus="[data-primary]">
  <p role={error ? "alert" : "status"}>{error ?? "Loading your settings…"}</p>
  {#snippet footer()}
    <Button onclick={onclose}>Close</Button>
    <Button variant="primary" icon="refresh" {loading} loadingText="Loading…" onclick={onretry} data-primary>Try again</Button>
  {/snippet}
</Dialog>
