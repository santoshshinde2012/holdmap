<script lang="ts">
  // Calm empty states: first setup, all clear, or nothing matches the filters.
  import EmptyPanel from "../ui/EmptyPanel.svelte";
  import Button from "../ui/Button.svelte";
  const TEXT = { setup: ["No analyzer is on", "Pick one to scan your agents' skills and MCP packages. Each asks first and runs sandboxed."], clear: ["No open findings", "Everything scanned looks fine."], nomatch: ["No findings match", ""] };
  let { kind, onprimary, onclear }: { kind: "setup" | "clear" | "nomatch"; onprimary?: () => void; onclear?: () => void } = $props();
</script>

<EmptyPanel icon={{ setup: "shield", clear: "check", nomatch: "filter" }[kind]} title={TEXT[kind][0]} hint={TEXT[kind][1]} tone={kind === "clear" ? "ok" : "accent"}>
  {#if kind === "setup" && onprimary}<Button size="sm" variant="primary" icon="plus" onclick={onprimary}>Choose an analyzer</Button>
  {:else if kind === "nomatch" && onclear}<Button size="sm" variant="ghost" onclick={onclear}>Clear filters</Button>{/if}
</EmptyPanel>
