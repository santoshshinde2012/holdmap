<script lang="ts">
  // Living reference for the UI kit (open the app with #ui-gallery). Used for visual review and
  // screenshots; not reachable from the product UI.
  import { Button, Callout, Checkbox, CopyValue, FilterChip, IconButton, Kbd, NumberInput, SegmentedControl, Select, Switch, TextField } from "../components/ui";

  let q = $state("shop-web");
  let host = $state("dev@buildbox:22x");
  let port = $state<number | null>(3000);
  let interval = $state<number | null>(4);
  let sort = $state("group");
  let seg = $state("tcp");
  let on = $state(true);
  let off = $state(false);
  let check = $state(true);
  let chip = $state(true);
  const noop = () => {};
</script>

<div class="gallery">
  <header><h1>Type &amp; controls</h1><p>Nine semantic text roles; one consistent set of inputs (32 px default, 28 px compact, 36 px large).</p></header>
  <section class="type" aria-label="Type scale">
    <h2>Type scale · Inter Variable + JetBrains Mono</h2>
    {#each [["display", "24 / 30 · 600", "3000"], ["title", "16 / 22 · 600", "Stop shop-web?"], ["heading", "14 / 20 · 600", "Global shortcut"], ["body", "13 / 20 · 400", "Port 3000 is held by Next.js (node, PID 43000)."], ["body-sm", "12 / 17 · 400", "Start portwise in the menu bar when you log in."], ["caption", "11 / 15 · 400", "Updated 2 s ago · 393 MB"], ["label", "11 / 15 · 500 · +6%", "DEV SERVERS"], ["mono", "12 / 18 · 400", "portwise stop 3000 --dry-run"], ["mono-sm", "11 / 16 · 400", "node · PID 43000 · 0O 1lI"]] as [r, spec, sample] (r)}
      <div class="spec"><span class="rn">{r}</span><span class="rs">{spec}</span><span class="sample t-{r}">{sample}</span></div>
    {/each}
  </section>
  <div class="grid">
    <section>
      <h2>Text fields</h2>
      <TextField label="Search" icon="search" placeholder="Search ports, projects, processes…" kbdHint="/" variant="filled" />
      <TextField label="Project" icon="folder" bind:value={q} clearable hint="Clear with the × or Esc." />
      <TextField label="SSH host" icon="terminal" mono bind:value={host} clearable error="The SSH port after “:” must be a number from 1 to 65535" />
      <TextField label="Label" optional placeholder="e.g. staging API" maxlength={40} counter value="shop web" />
      <TextField label="Disabled" disabled value="Not editable" />
    </section>
    <section>
      <h2>Numbers & choices</h2>
      <div class="row2">
        <NumberInput label="Port" required min={1} max={65535} bind:value={port} width="140px" hint="1–65535" />
        <NumberInput label="Scan every" min={1} max={60} unit="s" bind:value={interval} width="140px" />
      </div>
      <Select label="Sort by" bind:value={sort} options={[{ value: "group", label: "Grouped", description: "By kind" }, { value: "port", label: "Port" }, { value: "newest", label: "Newest" }]} />
      <div class="stack">
        <span class="lbl">Segmented control</span>
        <SegmentedControl label="Protocol" bind:value={seg} options={[{ value: "any", label: "Any" }, { value: "tcp", label: "TCP" }, { value: "udp", label: "UDP" }]} />
      </div>
      <div class="stack">
        <span class="lbl">Filter chips</span>
        <div class="flex"><FilterChip label="Dev servers" dot="var(--tone-green)" count={6} bind:pressed={chip} /><FilterChip label="Mine" icon="lock" count={9} /><FilterChip label="Exposed" icon="globe" tone="warn" count={3} pressed /></div>
      </div>
    </section>
    <section>
      <h2>Toggles</h2>
      <Switch label="Launch at login" description="Start in the menu bar when you log in." bind:checked={on} />
      <Switch label="Notifications" description="When a pinned port starts or stops." bind:checked={off} />
      <Switch label="Unavailable" description="Disabled state." disabled />
      <Checkbox label="Only dev servers" description="Skip databases and containers." bind:checked={check} />
      <Checkbox label="I understand — stop it anyway" tone="danger" />
    </section>
    <section>
      <h2>Buttons</h2>
      <div class="flex"><Button variant="primary" icon="star">Pin port</Button><Button>Cancel</Button><Button variant="ghost">Ghost</Button><Button variant="soft" icon="sparkles">Soft</Button></div>
      <div class="flex"><Button variant="danger" icon="stop" kbd="⌫">Stop</Button><Button variant="danger-outline" size="sm">Stop</Button><Button variant="primary" loading loadingText="Scanning…">Scan</Button><Button disabled>Disabled</Button></div>
      <div class="flex"><Button size="xs">XS 24</Button><Button size="sm">SM 28</Button><Button size="md">MD 32</Button><Button size="lg">LG 36</Button></div>
      <div class="flex"><IconButton icon="refresh" label="Refresh" kbd="R" /><IconButton icon="history" label="History" variant="outline" /><IconButton icon="star" label="Pin" pressed /><Kbd keys={["⌘", "K"]} /><Kbd keys="Esc" /></div>
      <CopyValue value="/Users/santosh/code/shop-web/node_modules/.bin/next-server" mono oncopy={noop} what="Path" />
      <Callout tone="warn" title="Reachable from your network">Bind to <code>127.0.0.1</code> if that isn't intended.</Callout>
    </section>
  </div>
</div>

<style>
  .gallery { min-height: 100vh; padding: 28px 32px; background: var(--bg); color: var(--text); }
  header h1 { margin: 0; font-size: var(--fs-title); font-weight: var(--fw-semibold); line-height: var(--lh-title); letter-spacing: var(--ls-title); }
  header p { margin: 4px 0 20px; color: var(--muted); font-size: var(--fs-body); line-height: var(--lh-body); }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }
  section { display: grid; gap: 14px; align-content: start; padding: 18px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); }
  h2 { margin: 0 0 2px; font-size: var(--fs-label); font-weight: var(--fw-medium); line-height: var(--lh-label); text-transform: uppercase; letter-spacing: var(--ls-label); color: var(--muted); }
  .row2, .flex { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
  .row2 { align-items: flex-start; gap: 16px; }
  .stack { display: grid; gap: 6px; justify-items: start; }
  .type { margin-bottom: 16px; gap: 2px; }
  .spec { display: grid; grid-template-columns: 90px 150px 1fr; align-items: baseline; gap: 12px; padding: 6px 0; border-top: 1px solid var(--border); }
  .rn { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--accent); }
  .rs { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .t-display { font-size: var(--fs-display); line-height: var(--lh-display); letter-spacing: var(--ls-display); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .t-title { font-size: var(--fs-title); line-height: var(--lh-title); letter-spacing: var(--ls-title); font-weight: var(--fw-semibold); }
  .t-heading { font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); }
  .t-body { font-size: var(--fs-body); line-height: var(--lh-body); }
  .t-body-sm { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); }
  .t-caption { font-size: var(--fs-caption); line-height: var(--lh-caption); letter-spacing: var(--ls-caption); color: var(--muted); }
  .t-label { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); font-weight: var(--fw-medium); color: var(--text-2); }
  .t-mono { font-family: var(--mono); font-size: var(--fs-mono); line-height: var(--lh-mono); }
  .t-mono-sm { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); }
  .lbl { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); }
</style>
