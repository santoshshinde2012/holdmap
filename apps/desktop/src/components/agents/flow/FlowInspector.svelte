<script lang="ts">
  import type { VisualGraph, VisualNode, VisualEdge } from "../../../lib/agent-flow/model";
  import { PROVIDER_LABEL, SOURCE_LABEL, STATUS_LABEL } from "../../../lib/agent-flow/model";
  import { duration, isIncomplete, payloadSize } from "../../../lib/agent-flow/build";
  import Icon from "../../Icon.svelte";
  let { graph, chosen, onclose }: { graph: VisualGraph; chosen: string | null; onclose: () => void } = $props();
  const node: VisualNode | undefined = $derived(graph.nodes.find((n) => n.id === chosen));
  const edge: VisualEdge | undefined = $derived(graph.edges.find((e) => e.id === chosen));
  const from = $derived(edge && graph.nodes.find((n) => n.id === edge.from));
  const to = $derived(edge && graph.nodes.find((n) => n.id === edge.to));
</script>

{#if node || edge}
  <section class="inspector" aria-label="Selected relationship details">
    <header><h3>{node?.label ?? edge?.label}</h3><button type="button" aria-label="Close relationship details" onclick={onclose}><Icon name="x" size={14} /></button></header>
    {#if node?.observed}
      {@const n = node.observed}
      <dl><dt>Type</dt><dd>{n.kind}</dd><dt>Evidence</dt><dd>{n.evidence}</dd>
        {#if n.pid != null}<dt>Process ID</dt><dd>{n.pid}</dd>{/if}
        {#if n.protocol}<dt>Protocol</dt><dd>{n.protocol.toUpperCase()}</dd>{/if}
        {#if n.address}<dt>Address</dt><dd>{n.address}</dd>{/if}
        {#if n.port != null}<dt>Port</dt><dd>{n.port}</dd>{/if}
        {#if n.tool_kind}<dt>Tool role</dt><dd>{n.tool_kind.replaceAll("_", " ")}</dd>{/if}
      </dl>
      <p>{n.evidence === "observed" ? "Observed in the operating system snapshot." : n.evidence === "inferred" ? "The tool role is inferred from process metadata. It does not prove a tool invocation." : "This identity could not be resolved from the snapshot."}</p>
    {:else if node?.span}
      {@const s = node.span}
      <dl><dt>Type</dt><dd>{s.kind}</dd><dt>Status</dt><dd>{STATUS_LABEL[s.status]}</dd><dt>Provider</dt><dd>{PROVIDER_LABEL[s.provider]}</dd><dt>Source</dt><dd>{SOURCE_LABEL[s.source]}</dd><dt>Duration</dt><dd>{duration(s)}</dd>
        <dt>Serialized input</dt><dd>{payloadSize(s.input_bytes)}</dd><dt>Serialized output</dt><dd>{payloadSize(s.output_bytes)}</dd>
        {#if s.pid != null}<dt>Reported PID</dt><dd>{s.pid} · unverified</dd>{/if}
      </dl>
      {#if s.kind === "tool" || s.kind === "model"}
        <div class="payload" aria-label="Reported serialized payload sizes"><span>Input<strong>{s.input_bytes == null ? "Unknown" : payloadSize(s.input_bytes)}</strong></span><i aria-hidden="true">→</i><span>{s.kind === "tool" ? "Tool" : "Model"}<strong>{STATUS_LABEL[s.status]}</strong></span><i aria-hidden="true">→</i><span>Result<strong>{s.output_bytes == null ? "Unknown" : payloadSize(s.output_bytes)}</strong></span></div>
      {/if}
      {#if isIncomplete(s)}<p class="warning">No completion has been received. A requested call may not have executed.</p>{/if}
      <p>Reported by the configured integration. Sizes describe reported serialized payloads, not network traffic. Only allowlisted metadata is retained; prompt and tool-argument fields are excluded. Names are sanitized source labels.</p>
    {:else if edge}
      <dl><dt>From</dt><dd>{from?.label ?? "Unknown"}</dd><dt>To</dt><dd>{to?.label ?? "Unknown"}</dd><dt>Evidence</dt><dd>{edge.kind === "span_parent" ? "Integration reported" : edge.evidence}</dd>
        {#if edge.observed?.kind === "tcp"}<dt>Live sockets</dt><dd>{edge.observed.connections}</dd><dt>Observed source PIDs</dt><dd>{edge.observed.source_pids.join(", ") || "Unavailable"}</dd>{/if}
      </dl>
      <p>{edge.kind === "tcp" ? "A live TCP socket relationship. Direction, messages and transferred bytes cannot be determined from this snapshot." : edge.kind === "parent" ? "A parent and child relationship observed in the process tree. It does not identify individual tool calls." : edge.kind === "owns_listener" ? "The operating system reports that this process holds the listening socket." : "The integration supplied this parent span ID. Session membership and event order alone never create this relationship."}</p>
    {/if}
  </section>
{/if}

<style>
  .inspector { position: sticky; top: 12px; z-index: 2; margin: 12px; padding: 12px; border: 1px solid var(--border-strong); border-radius: 11px; background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); }
  header { display: flex; gap: 8px; align-items: flex-start; justify-content: space-between; }
  h3 { margin: 0; font-size: var(--fs-body); overflow-wrap: anywhere; }
  button { display: inline-flex; color: var(--muted); border: 0; background: transparent; cursor: pointer; line-height: 1; padding: 2px; }
  button:focus-visible { outline: 2px solid var(--ring); }
  dl { display: grid; grid-template-columns: minmax(90px, auto) minmax(0, 1fr); gap: 5px 12px; font-size: var(--fs-caption); margin: 12px 0; }
  dt { color: var(--muted); } dd { margin: 0; overflow-wrap: anywhere; }
  p { margin: 8px 0 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-body); }
  .warning { color: var(--warn); }
  .payload { display: flex; align-items: center; gap: 5px; padding: 8px; margin: 8px 0; background: var(--surface-2); border-radius: 8px; font-size: var(--fs-caption); }
  .payload span { display: grid; gap: 3px; min-width: 0; flex: 1; color: var(--muted); }
  .payload strong { font-size: var(--fs-caption); color: var(--text-2); font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .payload i { font-style: normal; color: var(--muted); }
</style>
