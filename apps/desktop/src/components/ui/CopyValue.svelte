<script lang="ts">
  // A value that truncates with an ellipsis, shows the full text in a tooltip when clipped,
  // and has a copy button (revealed on hover / keyboard focus, always reachable by Tab).
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";

  let {
    value,
    display,
    mono = false,
    what = "Value",
    oncopy,
    wrap = false,
  }: {
    value: string;
    /** Text to show (defaults to `value`), e.g. a ~-shortened path. */
    display?: string;
    mono?: boolean;
    /** Noun for the toast and the button label ("Path", "Command"…). */
    what?: string;
    oncopy: (text: string, what: string) => void;
    /** Allow wrapping onto several lines instead of truncating. */
    wrap?: boolean;
  } = $props();

  let copied = $state(false);
  let t: ReturnType<typeof setTimeout> | undefined;
  function copy() {
    oncopy(value, what);
    copied = true;
    clearTimeout(t);
    t = setTimeout(() => (copied = false), 1400);
  }
</script>

<span class="cv" class:wrap>
  <span class="txt selectable" class:mono use:tooltip={wrap ? null : { text: value, mono, onlyIfTruncated: true, delay: 300 }}>{display ?? value}</span>
  <button type="button" class="cp" class:copied aria-label={copied ? `${what} copied` : `Copy ${what.toLowerCase()}`} use:tooltip={copied ? "Copied" : `Copy ${what.toLowerCase()}`} onclick={copy}>
    <Icon name={copied ? "check" : "copy"} size={12} />
  </button>
</span>

<style>
  .cv { display: flex; align-items: center; gap: 4px; min-width: 0; max-width: 100%; }
  .txt { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wrap .txt { white-space: normal; overflow-wrap: anywhere; }
  .mono { font-family: var(--mono); font-size: 12px; }
  .cp { flex: none; width: 22px; height: 22px; display: grid; place-items: center; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--muted); cursor: pointer; opacity: 0; transition: opacity var(--dur-1), background var(--dur-1), color var(--dur-1); }
  .cv:hover .cp, .cp:focus-visible, .cp.copied { opacity: 1; }
  .cp:hover { background: var(--surface-3); color: var(--text); }
  .cp.copied { color: var(--ok); }
  .cp:focus-visible { outline: 2px solid var(--ring); outline-offset: 0; }
  @media (hover: none) { .cp { opacity: 1; } }
</style>
