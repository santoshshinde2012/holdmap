<script lang="ts">
  import Facts, { type Fact } from "./Facts.svelte";
  import Section from "./Section.svelte";
  import Callout from "../ui/Callout.svelte";
  import RichText from "../ui/RichText.svelte";
  import Icon from "../Icon.svelte";
  import type { PortDetails, PortEntry } from "../../lib/types";
  import { canOpen, url } from "../../lib/format";

  let { entry, oncopy, details = null }: { entry: PortEntry; oncopy: (text: string, what: string) => void; details?: PortDetails | null } = $props();
  const exposure: Record<string, string> = { loopback: "This machine only", all_interfaces: "Every network interface", specific: "A specific interface" };
  const risk = $derived(details?.bind_risk ?? null);
  const conns = $derived(entry.protocol === "tcp" ? details?.connections ?? null : null);
  const states = $derived(conns ? Object.entries(conns.by_state).filter(([s]) => s !== "established").map(([s, n]) => `${n} ${s.replace(/_/g, " ")}`).join(" · ") : "");
  const facts = $derived.by((): Fact[] => [
    { label: "Protocol", value: entry.protocol.toUpperCase(), muted: entry.families?.length ? entry.families.map((f) => (f === "v4" ? "IPv4" : "IPv6")).join(" + ") : undefined },
    { label: "Bound to", value: entry.addresses.join(", "), mono: true, copy: true },
    { label: "Reachable from", value: exposure[entry.exposure] ?? entry.exposure, muted: risk ? `${risk.level} risk` : undefined },
    { label: "State", value: entry.state.replace(/_/g, " "), mono: true },
    { label: "URL", value: canOpen(entry) ? url(entry) : null, mono: true, copy: true },
    { label: "Remote", value: entry.remote ?? null, mono: true, copy: true },
    { label: "Tunnel", value: entry.tunnel ? `${entry.tunnel.kind} → ${entry.tunnel.target}` : null, mono: true, copy: true },
  ]);
</script>

<Section title="Socket" icon="globe"><Facts {facts} {oncopy} /></Section>
{#if risk && risk.level !== "low"}
  <div class="gap">
    <Callout tone={risk.level === "high" ? "danger" : "warn"} icon="globe" title={risk.title}>
      <p class="selectable"><RichText text={risk.explanation} /></p>
      {#if risk.fix}<p class="fix selectable"><strong>Fix:</strong> <RichText text={risk.fix} /></p>{/if}
    </Callout>
  </div>
{/if}

{#if conns}
  <Section title="Who's connected" icon="plug">
    {#snippet aside()}<span class="count">{conns.established} open{#if states}<span class="st"> · {states}</span>{/if}</span>{/snippet}
    {#if conns.peers.length}
      <ul class="peers" aria-label="Connected peers">
        {#each conns.peers as p (p.address + (p.pid ?? ""))}
          <li>
            <span class="ic" class:remote={!p.local}><Icon name={p.local ? "monitor" : "globe"} size={12} /></span>
            <span class="who selectable">{p.process ?? p.address}</span>
            <span class="addr selectable">{p.process ? `${p.address}${p.pid ? ` · PID ${p.pid}` : ""}` : p.local ? "this machine" : "another device"}</span>
            <span class="n">×{p.connections}</span>
          </li>
        {/each}
      </ul>
      {#if conns.more_peers}<p class="more">and {conns.more_peers} more</p>{/if}
    {:else}
      <p class="none">No one is connected right now.</p>
    {/if}
  </Section>
{/if}

<style>
  .gap { margin-top: var(--sp-3); }
  .gap + :global(section) { margin-top: var(--sp-6); }
  .fix { margin: 6px 0 0; }
  p { margin: 0; }
  .count { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); font-variant-numeric: tabular-nums; }
  .peers { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .peers li { display: flex; align-items: center; gap: 10px; min-height: 34px; padding: 6px 12px; border-top: 1px solid var(--border); min-width: 0; }
  .peers li:first-child { border-top: 0; }
  .ic { width: 22px; height: 22px; border-radius: var(--r-sm); display: grid; place-items: center; flex: none; color: var(--muted); background: var(--row-hover); }
  .ic.remote { color: var(--tone-amber); background: var(--tone-amber-bg); }
  .who { font-weight: var(--fw-medium); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .addr { color: var(--muted); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 1; }
  .n { margin-left: auto; font-family: var(--mono); font-size: var(--fs-mono-sm); color: var(--muted); font-variant-numeric: tabular-nums; }
  .none, .more { color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
</style>
