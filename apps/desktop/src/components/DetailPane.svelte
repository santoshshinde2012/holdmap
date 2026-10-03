<script lang="ts">
  const modKey = typeof navigator !== "undefined" && /Mac/.test(navigator.platform) ? "⌘" : "Ctrl";
  import { fade } from "svelte/transition";
  import Icon from "./Icon.svelte";
  import FrameworkIcon from "./FrameworkIcon.svelte";
  import type { Explanation, Graph, GraphNode, PortEntry, ProcRef } from "../lib/types";
  import { CLUSTER_LABEL, dependencies, dependents, edgeLabel, nodeForEntry } from "../lib/graph";
  import { canOpen, command, describeStep, humanBytes, tildify, title, uptime, url } from "../lib/format";

  let {
    entry,
    explanation,
    loading,
    busy,
    drawer = false,
    onstop,
    onkill,
    onopen,
    oncopy,
    onclose,
    graph = null,
    pinned = false,
    onpin,
    onstopcluster,
    onselectnode,
  }: {
    entry: PortEntry | null;
    explanation: Explanation | null;
    loading: boolean;
    busy: boolean;
    drawer?: boolean;
    onstop: () => void;
    onkill: () => void;
    onopen: () => void;
    oncopy: (text: string, what: string) => void;
    onclose?: () => void;
    graph?: Graph | null;
    pinned?: boolean;
    onpin?: () => void;
    onstopcluster?: (name: string) => void;
    onselectnode?: (n: GraphNode) => void;
  } = $props();

  const node = $derived(entry ? nodeForEntry(graph, entry.id) : null);
  const cluster = $derived(node?.cluster ? graph?.clusters.find((c) => c.id === node.cluster) ?? null : null);
  const deps = $derived(node && graph ? dependencies(graph, node.id) : []);
  const users = $derived(node && graph ? dependents(graph, node.id) : []);
  const edgeTo = (to: string) => graph?.edges.find((e) => e.from === node?.id && e.to === to);
  const edgeFrom = (from: string) => graph?.edges.find((e) => e.to === node?.id && e.from === from);

  const reduced = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const plan = $derived(explanation?.plan ?? null);
  const blocked = $derived(plan?.blocked ?? null);
  const chain = $derived.by((): ProcRef[] => {
    const s = plan?.steps.find((x) => x.action === "signal_processes");
    return s && s.action === "signal_processes" ? s.processes : [];
  });
  const stoppable = $derived(!!entry && !!(entry.process || entry.container) && !blocked);
  const blockTitle: Record<string, string> = {
    needs_elevation: "Needs administrator rights",
    os_service: "This is an operating-system feature",
    nothing_to_stop: "Nothing to stop",
    protected: "Protected process",
  };
</script>

<aside class="pane" class:drawer aria-label="Port details">
  {#if !entry}
    <div class="placeholder">
      <div class="ph-art" aria-hidden="true">
        <span class="ring r1"></span><span class="ring r2"></span><span class="core"><Icon name="radar" size={22} /></span>
      </div>
      <h3>Select a port</h3>
      <p>See who owns it, why it's busy, and exactly what “Stop” will do — before anything happens.</p>
      <div class="ph-keys"><span><kbd>↑</kbd><kbd>↓</kbd> to move</span><span aria-hidden="true">·</span><span><kbd>{modKey}</kbd><kbd>K</kbd> for commands</span></div>
    </div>
  {:else}
    {#key entry.id}
      <div class="inner" in:fade={{ duration: reduced ? 0 : 140 }}>
        <header class="head">
          <FrameworkIcon {entry} size={44} />
          <div class="who">
            <div class="hero"><span class="colon">:</span>{entry.port}<span class="proto">{entry.protocol}</span></div>
            <h2 title={title(entry)}>{title(entry)}{#if entry.framework && entry.framework.name !== title(entry)}<span class="fwn">· {entry.framework.name}</span>{/if}</h2>
          </div>
          {#if onclose}<button class="icon-btn close" aria-label="Close details" onclick={onclose}><Icon name="x" size={16} /></button>{/if}
        </header>

        <div class="chips">
          {#if entry.state === "listen" || entry.protocol === "udp"}<span class="badge tone-green"><span class="dot"></span>{entry.protocol === "udp" ? "Bound" : "Listening"}</span>{/if}
          {#if entry.project?.git_branch}<span class="badge"><Icon name="branch" size={11} />{entry.project.git_branch}</span>{/if}
          {#if entry.exposure === "all_interfaces"}<span class="badge tone-amber"><Icon name="globe" size={11} />Network-exposed</span>{:else if entry.exposure === "loopback"}<span class="badge"><Icon name="lock" size={11} />Local only</span>{/if}
          {#if entry.container}<span class="badge tone-blue"><Icon name="box" size={11} />{entry.container.runtime}</span>{/if}
          {#if entry.protected}<span class="badge"><Icon name="shield" size={11} />Protected</span>{/if}
        </div>

        <div class="actions">
          {#if entry.process || entry.container}
            <button class="btn danger" onclick={onstop} disabled={busy || !stoppable} aria-keyshortcuts="Backspace">
              {#if busy}<span class="spin"><Icon name="refresh" size={13} /></span>Stopping…{:else}<Icon name="stop" size={11} />Stop<kbd>⌫</kbd>{/if}
            </button>
            {#if entry.process && !entry.container}
              <button class="btn" onclick={onkill} disabled={busy || !stoppable} title="Skip SIGTERM and kill immediately (⇧⌫)"><Icon name="zap" size={13} />Force kill</button>
            {/if}
          {/if}
          {#if onpin}
            <button class="icon-btn pin" class:on={pinned} aria-pressed={pinned} title={pinned ? "Unpin :" + entry.port : "Pin :" + entry.port + " — keep it at the top and get notified when it changes"} aria-label={pinned ? "Unpin port" : "Pin port"} onclick={onpin}><Icon name="star" size={14} /></button>
          {/if}
          {#if canOpen(entry)}
            <button class="btn" onclick={onopen} aria-keyshortcuts="O"><Icon name="external" size={13} />Open</button>
            <button class="icon-btn" title="Copy {url(entry)}" aria-label="Copy URL" onclick={() => oncopy(url(entry), "URL")}><Icon name="copy" size={14} /></button>
          {/if}
        </div>

        <div class="scroll">
          {#if loading && !explanation}
            <div class="card"><div class="shimmer" style="height:14px;width:92%"></div><div class="shimmer" style="height:14px;width:64%;margin-top:10px"></div><div class="shimmer" style="height:44px;margin-top:16px"></div></div>
          {:else if explanation}
            <section class="card summary">
              <p class="headline selectable">{explanation.headline}</p>
              {#if blocked}
                <div class="callout warn" role="note">
                  <Icon name={blocked.kind === "needs_elevation" ? "lock" : "shield"} size={16} />
                  <div><strong>{blockTitle[blocked.kind]}</strong><p class="selectable">{blocked.message}</p></div>
                </div>
              {:else}
                <div class="callout tip">
                  <Icon name="sparkles" size={16} />
                  <div><strong>Recommended</strong><p class="selectable">{explanation.recommendation}</p></div>
                </div>
              {/if}
            </section>

            {#if node && (cluster || deps.length || users.length)}
              <section class="topo">
                <h4><Icon name="graph" size={12} />Connections</h4>
                {#if cluster}
                  <div class="cluster">
                    <Icon name="layers" size={13} />
                    <span><b>{cluster.name}</b> <span class="muted">{CLUSTER_LABEL[cluster.kind]} · {cluster.nodes.length} services</span></span>
                    {#if onstopcluster}<button class="btn sm" onclick={() => onstopcluster?.(cluster.name)} title="Stop every service in {cluster.name}, dependents first"><Icon name="stop" size={10} />Stop cluster</button>{/if}
                  </div>
                {/if}
                {#if deps.length}
                  <div class="rel-h">Depends on</div>
                  <ul class="rel">{#each deps as d}{@const e = edgeTo(d.id)}<li><button class="link" onclick={() => onselectnode?.(d)}><span class="arrow out">→</span>{d.label}</button><span class="mono muted">{e ? edgeLabel(e) : ""}</span></li>{/each}</ul>
                {/if}
                {#if users.length}
                  <div class="rel-h">Used by</div>
                  <ul class="rel">{#each users as d}{@const e = edgeFrom(d.id)}<li><button class="link" onclick={() => onselectnode?.(d)}><span class="arrow in">←</span>{d.label}</button><span class="mono muted">{e ? edgeLabel(e) : ""}</span></li>{/each}</ul>
                  <div class="callout warn subtle"><Icon name="alert" size={14} /><p>{users.length === 1 ? `${users[0].label} depends` : `${users.length} services depend`} on this — stopping it will break {users.length === 1 ? "it" : "them"}.</p></div>
                {/if}
              </section>
            {/if}

            {#if chain.length}
              <section>
                <h4><Icon name="tree" size={12} />What “Stop” will signal{#if plan}<span class="badge risk-{plan.risk}">{plan.risk} risk</span>{/if}</h4>
                <ol class="chain">
                  {#each chain as p, i}
                    <li class:holder={i === chain.length - 1}>
                      <span class="node"></span>
                      <div><span class="pname">{p.name}</span><span class="pid mono">PID {p.pid}</span>{#if i === chain.length - 1 && chain.length > 1}<span class="holds">holds :{entry.port}</span>{/if}
                        <div class="pcmd mono" title={p.command}>{p.command}</div></div>
                    </li>
                  {/each}
                </ol>
                {#if plan}
                  <ol class="steps">{#each plan.steps.filter((s) => s.action !== "signal_processes") as s}<li><Icon name="check" size={12} />{describeStep(s)}</li>{/each}</ol>
                  {#each plan.warnings as w}<div class="callout warn subtle"><Icon name="alert" size={14} /><p>{w}</p></div>{/each}
                {/if}
              </section>
            {:else if plan && !blocked && plan.steps.length}
              <section>
                <h4>What “Stop” will do <span class="badge risk-{plan.risk}">{plan.risk} risk</span></h4>
                <ol class="steps">{#each plan.steps as s}<li><Icon name="arrow" size={12} />{describeStep(s)}</li>{/each}</ol>
                {#each plan.warnings as w}<div class="callout warn subtle"><Icon name="alert" size={14} /><p>{w}</p></div>{/each}
              </section>
            {/if}

            {#if entry.process}
              <section>
                <h4>Process</h4>
                <dl>
                  <dt>Name</dt><dd>{entry.process.name} <span class="muted mono">PID {entry.process.pid}</span></dd>
                  {#if entry.user}<dt>User</dt><dd>{entry.user}{entry.is_mine ? "" : " · not you"}</dd>{/if}
                  {#if uptime(entry)}<dt>Uptime</dt><dd>{uptime(entry)}</dd>{/if}
                  {#if entry.process.memory_bytes}<dt>Memory</dt><dd>{humanBytes(entry.process.memory_bytes)}{#if node && node.pids.length > 1}<span class="muted"> · tree {humanBytes(node.memory_bytes)}</span>{/if}</dd>{/if}
                  {#if entry.process.cpu_percent !== undefined}<dt>CPU</dt><dd>{(node?.cpu_percent ?? entry.process.cpu_percent).toFixed(1)}%{#if node && node.pids.length > 1}<span class="muted"> · {node.pids.length} processes</span>{/if}</dd>{/if}
                  {#if entry.process.cwd}<dt>Directory</dt><dd class="mono selectable path">{tildify(entry.process.cwd)}</dd>{/if}
                </dl>
                <div class="codeblock">
                  <code class="selectable">{command(entry)}</code>
                  <button class="icon-btn tiny" aria-label="Copy command" title="Copy command" onclick={() => oncopy(command(entry), "Command")}><Icon name="copy" size={12} /></button>
                </div>
              </section>
            {/if}

            {#if entry.project}
              <section>
                <h4>Project</h4>
                <dl>
                  <dt>Name</dt><dd>{entry.project.name}</dd>
                  <dt>Path</dt><dd class="mono selectable path">{tildify(entry.project.root)}</dd>
                  <dt>Detected</dt><dd class="mono">{entry.project.kind}</dd>
                  {#if entry.project.workspace}<dt>Workspace</dt><dd>{entry.project.workspace.name} <span class="muted mono">{entry.project.workspace.kind}</span></dd>{/if}
                </dl>
              </section>
            {/if}

            {#if entry.container}
              <section>
                <h4>Container</h4>
                <dl>
                  <dt>Name</dt><dd>{entry.container.name}</dd>
                  <dt>Image</dt><dd class="mono">{entry.container.image}</dd>
                  <dt>Mapping</dt><dd class="mono">{entry.port} → {entry.container.private_port}</dd>
                  {#if entry.tunnel}<dt>Tunnel</dt><dd class="mono">{entry.tunnel.target}</dd>{/if}
                  {#if entry.container.compose_project}<dt>Compose</dt><dd class="mono">{entry.container.compose_project}/{entry.container.compose_service}</dd>{/if}
                </dl>
              </section>
            {/if}

            <section>
              <h4>Network</h4>
              <dl>
                {#if entry.tunnel && !entry.container}<dt>Tunnel</dt><dd class="mono selectable">{entry.tunnel.kind} → {entry.tunnel.target}</dd>{/if}
                <dt>Address</dt><dd class="mono selectable">{entry.addresses.join(", ")}</dd>
                <dt>State</dt><dd class="mono">{entry.state}</dd>
              </dl>
              {#if entry.exposure === "all_interfaces"}
                <div class="callout warn subtle"><Icon name="globe" size={14} /><p>Reachable from other devices on your network. Bind to <code>127.0.0.1</code> if that isn't intended.</p></div>
              {/if}
            </section>

            {#if explanation.commands.length}
              <section>
                <h4><Icon name="terminal" size={12} />From the terminal</h4>
                {#each explanation.commands as c}
                  <div class="codeblock shell">
                    <span class="prompt">$</span><code class="selectable">{c}</code>
                    <button class="icon-btn tiny" aria-label="Copy {c}" title="Copy" onclick={() => oncopy(c, "Command")}><Icon name="copy" size={12} /></button>
                  </div>
                {/each}
              </section>
            {/if}
          {/if}
        </div>
      </div>
    {/key}
  {/if}
</aside>

<style>
  .pane { display: flex; flex-direction: column; min-height: 0; height: 100%; background: var(--surface); border-left: 1px solid var(--border); }
  .pane.drawer { box-shadow: var(--shadow-lg); border-left: 0; }
  .inner { display: flex; flex-direction: column; min-height: 0; height: 100%; }

  .placeholder { margin: auto; text-align: center; max-width: 280px; color: var(--muted); padding: var(--sp-6); }
  .placeholder h3 { color: var(--text); margin: var(--sp-5) 0 var(--sp-1); font-size: var(--fs-md); }
  .placeholder p { margin: 0 0 var(--sp-4); }
  .ph-keys { display: inline-flex; flex-wrap: wrap; justify-content: center; align-items: center; gap: 4px; white-space: nowrap; font-size: var(--fs-xs); }
  .ph-art { position: relative; width: 84px; height: 84px; margin: 0 auto; display: grid; place-items: center; }
  .ring { position: absolute; inset: 0; border-radius: 50%; border: 1px solid var(--accent); opacity: 0; animation: ping 2.8s var(--ease) infinite; }
  .ring.r2 { animation-delay: 1.4s; }
  .core { width: 48px; height: 48px; border-radius: 16px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent); }
  @keyframes ping { 0% { transform: scale(0.55); opacity: 0.6; } 100% { transform: scale(1.15); opacity: 0; } }

  .head { display: flex; gap: var(--sp-3); align-items: center; padding: var(--sp-5) var(--sp-5) var(--sp-3); }
  .who { min-width: 0; flex: 1; }
  .hero { font-family: var(--mono); font-size: var(--fs-hero); font-weight: 700; letter-spacing: -0.04em; line-height: 1; display: flex; align-items: baseline; }
  .colon { color: var(--faint); margin-right: -0.14em; }
  .fwn { margin-left: 6px; }
  .hero .proto { font-family: var(--font); font-size: var(--fs-2xs); text-transform: uppercase; color: var(--muted); letter-spacing: 0.08em; font-weight: 650; margin-left: var(--sp-2); }
  .who h2 { margin: 6px 0 0; font-size: var(--fs-md); font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fwn { color: var(--muted); font-weight: 500; }
  .close { align-self: flex-start; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 var(--sp-5) var(--sp-3); }
  .actions { display: flex; gap: var(--sp-2); padding: var(--sp-1) var(--sp-5) var(--sp-4); border-bottom: 1px solid var(--border); flex-wrap: wrap; align-items: center; }

  .scroll { overflow-y: auto; padding: var(--sp-4) var(--sp-5) var(--sp-8); flex: 1; display: flex; flex-direction: column; gap: var(--sp-5); }
  .card { background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-lg); padding: var(--sp-4); }
  .summary { display: grid; gap: var(--sp-3); }
  section h4 { margin: 0 0 var(--sp-2); font-size: var(--fs-2xs); text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); font-weight: 700; display: flex; align-items: center; gap: 6px; }
  section h4 .badge { margin-left: auto; text-transform: none; letter-spacing: 0; }
  .headline { font-size: var(--fs-md); margin: 0; color: var(--text); line-height: 1.5; font-weight: 500; }
  dl { display: grid; grid-template-columns: 84px minmax(0, 1fr); gap: 7px var(--sp-3); margin: 0; font-size: var(--fs-sm); }
  dt { color: var(--muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .muted { color: var(--muted); }
  .path { color: var(--text-2); }
  .codeblock { display: flex; gap: var(--sp-2); align-items: flex-start; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-sm); padding: 6px 4px 6px 10px; margin-top: var(--sp-2); }
  .codeblock code { flex: 1; overflow-wrap: anywhere; max-height: 76px; overflow-y: auto; padding-top: 3px; color: var(--text-2); }
  .codeblock.shell code { white-space: nowrap; overflow-x: auto; overflow-wrap: normal; }
  .prompt { color: var(--faint); font-family: var(--mono); padding-top: 3px; }
  .tiny { width: 24px; height: 24px; flex: none; }

  .callout { display: flex; gap: 10px; padding: 10px 12px; border-radius: var(--r-md); font-size: var(--fs-sm); align-items: flex-start; }
  .callout p { margin: 0; }
  .callout strong { display: block; margin-bottom: 2px; font-size: var(--fs-xs); text-transform: uppercase; letter-spacing: 0.05em; }
  .callout :global(svg) { margin-top: 1px; flex: none; }
  .callout.tip { background: var(--accent-soft); }
  .callout.tip strong, .callout.tip :global(svg) { color: var(--accent); }
  .callout.warn { background: var(--warn-soft); }
  .callout.warn strong, .callout.warn :global(svg) { color: var(--warn); }
  .callout.subtle { padding: 8px 10px; font-size: var(--fs-xs); margin-top: var(--sp-2); }

  .chain { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: minmax(0, 1fr); gap: 0; }
  .chain li { display: flex; min-width: 0; gap: var(--sp-3); position: relative; padding: 6px 0 6px 2px; }
  .chain li:not(:last-child)::after { content: ""; position: absolute; left: 7px; top: 22px; bottom: -6px; width: 2px; background: var(--border-strong); border-radius: 2px; }
  .node { flex: none; width: 12px; height: 12px; margin-top: 3px; border-radius: 50%; border: 2px solid var(--border-strong); background: var(--surface); position: relative; z-index: 1; }
  .holder .node { border-color: var(--accent); background: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .chain > li > div { min-width: 0; flex: 1; }
  .pname { font-weight: 600; }
  .pid { color: var(--muted); margin-left: 8px; font-size: 11px; }
  .holds { margin-left: 8px; font-size: var(--fs-2xs); color: var(--accent); font-weight: 650; text-transform: uppercase; letter-spacing: 0.05em; }
  .pcmd { color: var(--muted); font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
  .steps { list-style: none; margin: var(--sp-2) 0 0; padding: 0; display: grid; gap: 6px; color: var(--text-2); font-size: var(--fs-sm); }
  .steps li { display: flex; gap: var(--sp-2); align-items: flex-start; }
  .steps :global(svg) { color: var(--ok); margin-top: 3px; flex: none; }
  .pin.on { color: var(--warn); }
  .pin.on :global(svg) { fill: currentColor; }
  .topo .cluster { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: var(--r-md); background: var(--accent-soft); color: var(--accent); font-size: var(--fs-sm); }
  .topo .cluster > span { flex: 1; color: var(--text); min-width: 0; }
  .rel-h { margin: var(--sp-3) 0 4px; font-size: var(--fs-xs); color: var(--muted); font-weight: 600; }
  .rel { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .rel li { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: var(--fs-sm); }
  .link { border: 0; background: none; padding: 3px 0; color: var(--text); font: inherit; font-weight: 600; cursor: pointer; display: inline-flex; gap: 8px; align-items: center; }
  .link:hover { color: var(--accent); }
  .arrow { font-family: var(--mono); font-weight: 700; }
  .arrow.out { color: var(--tone-green); }
  .arrow.in { color: var(--warn); }
  .risk-low { color: var(--ok); background: var(--ok-soft); }
  .risk-medium { color: var(--warn); background: var(--warn-soft); }
  .risk-high { color: var(--danger); background: var(--danger-soft); }
</style>
