<script lang="ts">
  import Facts, { type Fact } from "./Facts.svelte";
  import Section from "./Section.svelte";
  import Callout from "../ui/Callout.svelte";
  import type { PortEntry } from "../../lib/types";
  import { canOpen, url } from "../../lib/format";

  let { entry, oncopy }: { entry: PortEntry; oncopy: (text: string, what: string) => void } = $props();
  const exposure: Record<string, string> = { loopback: "This machine only", all_interfaces: "Every network interface", specific: "A specific interface" };
  const facts = $derived.by((): Fact[] => [
    { label: "Protocol", value: entry.protocol.toUpperCase(), muted: entry.families?.length ? entry.families.map((f) => (f === "v4" ? "IPv4" : "IPv6")).join(" + ") : undefined },
    { label: "Bound to", value: entry.addresses.join(", "), mono: true, copy: true },
    { label: "Reachable from", value: exposure[entry.exposure] ?? entry.exposure },
    { label: "State", value: entry.state.replace(/_/g, " "), mono: true },
    { label: "URL", value: canOpen(entry) ? url(entry) : null, mono: true, copy: true },
    { label: "Remote", value: entry.remote ?? null, mono: true, copy: true },
    { label: "Tunnel", value: entry.tunnel ? `${entry.tunnel.kind} → ${entry.tunnel.target}` : null, mono: true, copy: true },
  ]);
</script>

<Section title="Socket" icon="globe"><Facts {facts} {oncopy} /></Section>
{#if entry.exposure === "all_interfaces"}
  <div class="gap"><Callout tone="warn" icon="globe" title="Reachable from your network">Other devices on your network can connect. Bind to <code>127.0.0.1</code> if that isn't intended.</Callout></div>
{/if}

<style>.gap { margin-top: var(--sp-3); }</style>
