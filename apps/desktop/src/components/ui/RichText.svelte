<script lang="ts">
  // Renders core prose with `backtick` segments as inline code (no HTML injection: text only).
  import { codeSpans } from "../../lib/text";
  let { text }: { text: string } = $props();
</script>

{#each codeSpans(text) as s, i (i)}{#if s.code}<code>{s.text}</code>{:else}{s.text}{/if}{/each}
