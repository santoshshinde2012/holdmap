// First-run choices: what the welcome dialog offers and how a choice turns into settings.
// Skipping changes nothing except remembering that the welcome was seen.
export type StartView = "home" | "agents" | "list";

export interface FirstRunChoices {
  start: StartView;
  /** Desktop notifications for new and conflicting listeners. */
  notify: boolean;
  /** Start holdmap at login (desktop builds only). */
  autostart: boolean;
  /** One-key shortcuts such as G, ⇧A and ⌫. */
  singleKeys: boolean;
  /** Read agents' MCP, skills and permission files (names and metadata only). */
  inventory: boolean;
}

export interface CurrentSettings { notify: boolean; autostart: boolean; singleKeys: boolean; inventory: boolean }

/** The dialog's initial state: start on the overview; integrations show their current settings. */
export function defaultChoices(current: CurrentSettings): FirstRunChoices {
  return { start: "home", notify: current.notify, autostart: current.autostart, singleKeys: current.singleKeys, inventory: current.inventory };
}

export type FirstRunChange =
  | { kind: "view"; view: StartView }
  | { kind: "notify"; on: boolean }
  | { kind: "autostart"; on: boolean }
  | { kind: "singleKeys"; on: boolean }
  | { kind: "inventory"; on: boolean };

/** Only what differs from the current settings, so applying a plan is idempotent. */
export function firstRunPlan(choices: FirstRunChoices, current: CurrentSettings, canAutostart: boolean): FirstRunChange[] {
  const out: FirstRunChange[] = [{ kind: "view", view: choices.start }];
  if (choices.notify !== current.notify) out.push({ kind: "notify", on: choices.notify });
  if (canAutostart && choices.autostart !== current.autostart) out.push({ kind: "autostart", on: choices.autostart });
  if (choices.singleKeys !== current.singleKeys) out.push({ kind: "singleKeys", on: choices.singleKeys });
  if (choices.inventory !== current.inventory) out.push({ kind: "inventory", on: choices.inventory });
  return out;
}
