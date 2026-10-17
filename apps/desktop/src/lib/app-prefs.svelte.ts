// The app's remembered choices: each is reactive state read once from `holdmap.*` and written
// back whenever it changes. Create it during component setup so the write-back effects run.
import type { View } from "../components/shell/ViewToolbar.svelte";
import type { Picture } from "./agents";
import { HOME_LAYOUTS, type HomeLayout } from "./home-layout";
import { prefGet, prefGetBoolean, prefGetChoice, prefSet } from "./prefs";
import { parseCollapsed, parseDensity, type Density } from "./rows";
import { SORTS, type Sort } from "./port-list";
import type { Theme } from "./settings";

export const VIEWS: readonly View[] = ["home", "list", "graph", "agents"];
export const AGENT_SECTIONS = ["overview", "flow", "capabilities", "insights"] as const;
export type AgentSection = (typeof AGENT_SECTIONS)[number];
export const FLOW_SOURCES = ["connections", "calls"] as const;
export type FlowSource = (typeof FLOW_SOURCES)[number];
export const PANE_DEFAULT = 460;
export const PANE_MIN = 340;
export const PANE_MAX = 680;

const flag = (on: boolean) => (on ? "1" : "0");

export class AppPrefs {
  view = $state<View>(prefGetChoice("view", VIEWS, "list"));
  theme = $state<Theme>(prefGetChoice("theme", ["system", "light", "dark"], "system"));
  density = $state<Density>(parseDensity(prefGet("density")));
  singleKeyShortcuts = $state(prefGetBoolean("singleKeyShortcuts", true));
  sort = $state<Sort>(prefGetChoice("sort", SORTS, "group"));
  collapsed = $state<Set<string>>(parseCollapsed(prefGet("collapsed")));
  homeLayout = $state<HomeLayout>(prefGetChoice("home.layout", HOME_LAYOUTS, "cards"));
  homeTimeline = $state(prefGetBoolean("home.timeline", true));
  agentSection = $state<AgentSection>(prefGetChoice("agents.section", AGENT_SECTIONS, "overview"));
  inventoryOn = $state(prefGetBoolean("agents.inventory", false));
  flowSource = $state<FlowSource>(prefGetChoice("flow.source", FLOW_SOURCES, "connections"));
  // "project" needs a selected folder, so a restart opens the computer picture instead.
  picture = $state<Picture>(prefGetChoice("agents.picture", ["context", "footprint", "project"] as const, "context") === "footprint" ? "footprint" : "context");
  paneWidth = $state(Math.min(PANE_MAX, Math.max(PANE_MIN, Number(prefGet("pane")) || PANE_DEFAULT)));
  sideOpen = $state(prefGetBoolean("side", true));

  constructor() {
    $effect(() => prefSet("view", this.view));
    $effect(() => prefSet("theme", this.theme));
    $effect(() => prefSet("density", this.density));
    $effect(() => prefSet("singleKeyShortcuts", flag(this.singleKeyShortcuts)));
    $effect(() => prefSet("sort", this.sort));
    $effect(() => prefSet("collapsed", JSON.stringify([...this.collapsed])));
    $effect(() => prefSet("home.layout", this.homeLayout));
    $effect(() => prefSet("home.timeline", flag(this.homeTimeline)));
    $effect(() => prefSet("agents.section", this.agentSection));
    $effect(() => prefSet("agents.inventory", flag(this.inventoryOn)));
    $effect(() => prefSet("flow.source", this.flowSource));
    $effect(() => prefSet("agents.picture", this.picture));
    $effect(() => prefSet("pane", String(this.paneWidth)));
    $effect(() => prefSet("side", flag(this.sideOpen)));
  }
}
