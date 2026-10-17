import { describe, expect, it } from "vitest";
import { defaultChoices, firstRunPlan } from "./onboarding";

const current = { notify: false, autostart: false, singleKeys: true, inventory: false };

describe("first-run choices", () => {
  it("defaults to the overview and keeps integrations as they are", () => {
    expect(defaultChoices(current)).toEqual({ start: "home", notify: false, autostart: false, singleKeys: true, inventory: false });
  });

  it("plans only the settings that change", () => {
    expect(firstRunPlan(defaultChoices(current), current, true)).toEqual([{ kind: "view", view: "home" }]);
    expect(firstRunPlan({ start: "agents", notify: true, autostart: true, singleKeys: false, inventory: true }, current, true)).toEqual([
      { kind: "view", view: "agents" },
      { kind: "notify", on: true },
      { kind: "autostart", on: true },
      { kind: "singleKeys", on: false },
      { kind: "inventory", on: true },
    ]);
  });

  it("never asks for launch at login where it isn't available", () => {
    expect(firstRunPlan({ ...defaultChoices(current), autostart: true }, current, false).some((c) => c.kind === "autostart")).toBe(false);
  });
});
