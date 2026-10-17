// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { focusables, focusTrap, trapTab } from "./focus";

afterEach(() => { document.body.replaceChildren(); });

function surface() {
  const root = document.createElement("div"), button = document.createElement("button");
  root.append(button);
  document.body.append(root);
  return { root, button };
}

describe("dialog tab order", () => {
  it("excludes roving controls, disabled controls and hidden ancestors", () => {
    const { root, button } = surface();
    root.insertAdjacentHTML("beforeend", '<button tabindex="-1">Inactive tab</button><button tabindex="-2">Programmatic focus</button><button disabled tabindex="0">Disabled</button><fieldset disabled><input></fieldset><div hidden><button>Hidden</button></div><div aria-hidden="true"><button>Decorative</button></div><div inert><button>Inert</button></div><div style="display:none"><button>Collapsed</button></div><button style="visibility:hidden">Invisible</button>');
    expect(focusables(root)).toEqual([button]);
    button.focus();
    const forward = new KeyboardEvent("keydown", { key: "Tab", cancelable: true });
    expect(trapTab(forward, root)).toBe(true);
    expect(forward.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(button);
    const backward = new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, cancelable: true });
    expect(trapTab(backward, root)).toBe(true);
    expect(document.activeElement).toBe(button);
  });
  it("leaves consumed, composing and modified Tab keys alone", () => {
    const { root, button } = surface();
    button.focus();
    for (const extra of [{ isComposing: true }, { keyCode: 229 }, { altKey: true }, { ctrlKey: true }, { metaKey: true }]) {
      const event = new KeyboardEvent("keydown", { key: "Tab", cancelable: true, ...extra });
      expect(trapTab(event, root)).toBe(false);
      expect(event.defaultPrevented).toBe(false);
    }
    const consumed = new KeyboardEvent("keydown", { key: "Tab", cancelable: true });
    consumed.preventDefault();
    expect(trapTab(consumed, root)).toBe(false);
  });
});

describe("dialog focus when a control removes itself", () => {
  it("moves focus to the surface so Esc and Tab still reach the dialog", async () => {
    const { root, button } = surface();
    root.tabIndex = -1;
    const trap = focusTrap(root);
    await Promise.resolve();
    button.focus();
    button.remove();
    await Promise.resolve();
    expect(document.activeElement).toBe(root);
    trap.destroy();
  });
  it("leaves focus alone when it already moved somewhere else", async () => {
    const { root, button } = surface();
    const other = document.createElement("button");
    root.append(other);
    const trap = focusTrap(root);
    await Promise.resolve();
    button.focus();
    other.focus();
    button.remove();
    await Promise.resolve();
    expect(document.activeElement).toBe(other);
    trap.destroy();
  });
});

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
