<script lang="ts">
  // Live readings over the 3D machine: the same figures the scene is built from, for the whole
  // computer or the agent in focus. A figure that wasn't collected reads "Not read", never 0.
  // Readings that count parts are toggles: pressing one lights up exactly those parts.
  import { sparkPoints } from "../../../lib/rows";
  import { isGauge, type Gauge, type Reading } from "../../../lib/machine3d/scene";

  let { readings, scope, cpu = [], active = null, flat = false, titled = true, onpick }: {
    readings: Reading[];
    /** What the figures cover: "This computer" or an agent's name. */
    scope: string;
    /** Recent CPU samples for the CPU trend. */
    cpu?: number[];
    /** The reading whose parts are lit, if any. */
    active?: Gauge | null;
    /** Inside a panel that already frames it (Home's Details), rather than floating over the canvas. */
    flat?: boolean;
    /** The visible title. Off when the panel around it already names the same thing. */
    titled?: boolean;
    onpick?: (gauge: Gauge | null) => void;
  } = $props();
  const spark = $derived(sparkPoints(cpu, 120, 26, 5));
</script>

<section class="readings" class:flat aria-label="Live readings: {scope}">
  {#if titled}<h3>{scope}</h3>{/if}
  <ul>
    {#each readings as r (r.id)}
      {@const gauge = isGauge(r.id) && r.value !== null && r.value !== "…" && !!onpick}
      {@const note = !titled && r.sub === "this agent" ? "" : r.sub}
      <li class="r" class:cpu={r.id === "cpu"} class:warn={r.tone === "warn"} class:ok={r.tone === "ok"} class:unknown={r.value === null}>
        {#if gauge}
          <button type="button" class="tile" aria-pressed={active === r.id} title="Light up these parts" onclick={() => onpick?.(active === r.id ? null : r.id as Gauge)}>
            <span class="v">{r.value}</span>{" "}<span class="l">{r.label}</span>{#if note}{" "}<small>{note}</small>{/if}
          </button>
        {:else}
          <div class="tile">
            <span class="v">{r.value ?? "Not read"}</span>{" "}<span class="l">{r.label}</span>{#if note}{" "}<small>{note}</small>{/if}
            {#if r.id === "cpu" && spark}<svg class="spark" viewBox="0 0 120 26" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark} /></svg>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  .readings { pointer-events: auto; padding: 7px 8px 6px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 90%, transparent); box-shadow: var(--shadow-sm); backdrop-filter: blur(6px); }
  .readings.flat { padding: 8px 6px 6px; border: 0; border-radius: 0; background: transparent; box-shadow: none; backdrop-filter: none; }
  h3 { margin: 0 0 2px 4px; font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-semibold); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(104px, 1fr)); gap: 2px 4px; }
  .r { min-width: 0; }
  .r.unknown { grid-column: 1 / -1; }
  .unknown .tile { grid-template-columns: minmax(0, 1fr) auto; grid-template-areas: "l v" "sub sub"; align-items: baseline; column-gap: 8px; padding-top: 2px; padding-bottom: 2px; }
  /* The number stands alone, then its name, then what it covers. Nothing is cut off mid-word. */
  .tile { display: grid; grid-template-areas: "v" "l" "sub" "spark"; align-content: start; gap: 0;
    width: 100%; min-width: 0; padding: 4px 6px 5px; border: 1px solid transparent; border-radius: var(--r-md); background: transparent; color: inherit; font: inherit; text-align: left; }
  button.tile { cursor: pointer; }
  button.tile:hover { background: var(--row-hover); }
  button.tile[aria-pressed="true"] { background: var(--accent-soft); border-color: color-mix(in srgb, var(--accent) 40%, transparent); }
  button.tile:focus-visible { outline: 2px solid var(--ring); outline-offset: 0; }
  .l { grid-area: l; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .v { grid-area: v; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  .unknown .v { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); color: var(--muted); }
  .warn .v { color: var(--warn); }
  .ok .v { color: var(--ok); }
  small { grid-area: sub; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .spark { grid-area: spark; width: 100%; height: 16px; margin-top: 2px; overflow: hidden; }
  .spark polyline { fill: none; stroke: var(--accent); stroke-width: 1.5; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
</style>
