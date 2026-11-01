// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import CollapsiblePanel from "./CollapsiblePanel.svelte";

afterEach(() => { cleanup(); localStorage.clear(); });
const body = createRawSnippet(() => ({ render: () => "<p>Inside</p>" }));

describe("CollapsiblePanel", () => {
  it("folds under a heading that holds its toggle, and remembers it", async () => {
    const view = render(CollapsiblePanel, { id: "t", title: "Needs you", count: 3, pref: "test.panel", children: body });
    const heading = view.getByRole("heading", { name: /^Needs you 3/ });
    const toggle = view.getByRole("button", { name: /Needs you/ });
    expect(heading.contains(toggle)).toBe(true);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(view.getByText("Inside")).toBeTruthy();
    await fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(view.container.querySelector("#t-b")!.hasAttribute("hidden")).toBe(true);
    cleanup();
    // Remembered across a remount.
    const again = render(CollapsiblePanel, { id: "t", title: "Needs you", pref: "test.panel", children: body });
    expect(again.getByRole("button", { name: /Needs you/ }).getAttribute("aria-expanded")).toBe("false");
  });

  it("closes only when its owner allows it", async () => {
    let closed = 0;
    const view = render(CollapsiblePanel, { id: "c", title: "Details", onclose: () => closed++, children: body });
    await fireEvent.click(view.getByRole("button", { name: "Close Details" }));
    expect(closed).toBe(1);
    cleanup();
    expect(render(CollapsiblePanel, { id: "d", title: "Details", children: body }).queryByRole("button", { name: "Close Details" })).toBeNull();
  });
});
