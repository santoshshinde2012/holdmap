// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { focusTrap } from "./focus";

afterEach(() => { document.body.replaceChildren(); });

function surface() {
  const root = document.createElement("div"), button = document.createElement("button");
  root.append(button);
  document.body.append(root);
  return { root, button };
}

describe("dialog focus restoration", () => {
  it("returns to the original trigger after replacing a loading dialog with its review", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const loading = surface(), loadingTrap = focusTrap(loading.root);
    await Promise.resolve();
    expect(document.activeElement).toBe(loading.button);
    const review = surface(), reviewTrap = focusTrap(review.root);
    loading.root.remove();
    loadingTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).toBe(review.button);
    review.root.remove();
    reviewTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).toBe(trigger);
  });

  it("returns to the parent dialog while it remains open", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const parent = surface(), parentTrap = focusTrap(parent.root);
    await Promise.resolve();
    const nested = surface(), nestedTrap = focusTrap(nested.root);
    await Promise.resolve();
    nested.root.remove();
    nestedTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).toBe(parent.button);
    parent.root.remove();
    parentTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).toBe(trigger);
  });

  it("preserves the trigger when an outgoing surface is removed before its replacement mounts", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const loading = surface(), loadingTrap = focusTrap(loading.root);
    await Promise.resolve();
    loading.root.remove();
    loadingTrap.destroy();
    expect(document.activeElement).toBe(document.body);
    const review = surface(), reviewTrap = focusTrap(review.root);
    await Promise.resolve();
    expect(document.activeElement).toBe(review.button);
    review.root.remove();
    reviewTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).toBe(trigger);
  });

  it("honors restore=false and never focuses an inert previous trigger", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const dialog = surface(), trap = focusTrap(dialog.root, { restore: false });
    await Promise.resolve();
    dialog.root.remove();
    trap.destroy();
    await Promise.resolve();
    expect(document.activeElement).not.toBe(trigger);
    trigger.focus();
    const next = surface(), nextTrap = focusTrap(next.root);
    await Promise.resolve();
    trigger.setAttribute("inert", "");
    next.root.remove();
    nextTrap.destroy();
    await Promise.resolve();
    expect(document.activeElement).not.toBe(trigger);
  });
});
