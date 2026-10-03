// @vitest-environment jsdom
// The port list row and section header: semantics, no-layout-shift actions, badge cap, collapse.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import PortRow from "./PortRow.svelte";
import GroupHeader from "./GroupHeader.svelte";
import { MOCK_SNAPSHOT } from "../../lib/mock";

beforeAll(() => {
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
});
afterEach(cleanup);
const byPort = (p: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === p)!;
const handlers = () => ({ onselect: vi.fn(), onstop: vi.fn(), onopen: vi.fn(), onpin: vi.fn() });

describe("PortRow", () => {
  it("is an option with aria-selected, position and a descriptive name", () => {
    render(PortRow, { entry: byPort(8000), selected: true, busy: false, posinset: 3, setsize: 10, ...handlers() });
    const row = screen.getByRole("option");
    expect(row.getAttribute("aria-selected")).toBe("true");
    expect(row.getAttribute("aria-posinset")).toBe("3");
    expect(row.getAttribute("aria-setsize")).toBe("10");
    expect(row.getAttribute("aria-label")).toContain("Port 8000 tcp, ml-service");
    expect(row.getAttribute("tabindex")).toBe("-1"); // focus stays on the listbox (aria-activedescendant)
  });

  it("renders every column in a fixed order so the grid lines up across rows", () => {
    const { container } = render(PortRow, { entry: byPort(3000), selected: false, busy: false, ...handlers() });
    const cols = [...container.querySelector(".row")!.children].map((c) => c.className.split(" ").find((x) => !x.startsWith("s-") && !x.startsWith("svelte-")));
    expect(cols).toEqual(["dot", "port", "name", "meta", "col-badges", "usage", "age", "actions"]);
  });

  it("caps badges at two chips with a +N overflow", () => {
    const { container } = render(PortRow, { entry: byPort(5432), links: 2, selected: false, busy: false, ...handlers() });
    const chips = container.querySelectorAll(".chip");
    expect(chips.length).toBe(2);
    expect(chips[1].textContent).toBe("+2");
    expect(chips[1].getAttribute("aria-label")).toBe("2 more: Docker · 2 connected");
  });

  it("actions overlay the row (absolutely positioned, not a grid column) and don't select the row", async () => {
    const h = handlers();
    const { container } = render(PortRow, { entry: byPort(3000), pinned: true, selected: false, busy: false, ...h });
    const actions = container.querySelector(".actions")!;
    expect(actions.parentElement!.classList.contains("row")).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Stop port 3000" }));
    await fireEvent.click(screen.getByRole("button", { name: "Unpin" }));
    await fireEvent.click(screen.getByRole("button", { name: "Open in browser" }));
    expect(h.onstop).toHaveBeenCalledOnce();
    expect(h.onpin).toHaveBeenCalledOnce();
    expect(h.onopen).toHaveBeenCalledOnce();
    expect(h.onselect).not.toHaveBeenCalled();
    for (const b of actions.querySelectorAll("button")) expect(b.getAttribute("tabindex")).toBe("-1");
  });

  it("hides Stop for protected ports and disables it while stopping", () => {
    render(PortRow, { entry: byPort(5000), selected: false, busy: false, ...handlers() });
    expect(screen.queryByRole("button", { name: /Stop port/ })).toBeNull();
    cleanup();
    render(PortRow, { entry: byPort(3001), selected: false, busy: true, ...handlers() });
    expect((screen.getByRole("button", { name: "Stopping…" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("img", { name: "Stopping…" })).toBeTruthy();
  });

  it("marks the density on the row", () => {
    const { container } = render(PortRow, { entry: byPort(3001), density: "compact", selected: false, busy: false, ...handlers() });
    expect(container.querySelector(".row")!.classList.contains("compact")).toBe(true);
  });
});

describe("GroupHeader", () => {
  it("toggles with aria-expanded, names its count, and controls the group", async () => {
    const ontoggle = vi.fn();
    const { rerender } = render(GroupHeader, { id: "dev", title: "Dev servers", count: 4, hint: "Your running projects", ontoggle });
    const btn = screen.getByRole("button");
    expect(btn.getAttribute("aria-expanded")).toBe("true");
    expect(btn.getAttribute("aria-controls")).toBe("grp-dev");
    expect(screen.getByLabelText("4 ports")).toBeTruthy();
    expect(screen.queryByText("Your running projects")).toBeNull(); // generic blurb is a tooltip, not clutter
    await fireEvent.click(btn);
    expect(ontoggle).toHaveBeenCalledOnce();
    await rerender({ id: "dev", title: "Dev servers", count: 4, collapsed: true, ontoggle });
    expect(screen.getByRole("button").getAttribute("aria-expanded")).toBe("false");
  });
  it("shows cluster detail inline", () => {
    render(GroupHeader, { id: "c1", title: "shop", count: 3, hint: "Compose · docker compose", showHint: true, ontoggle: () => {} });
    expect(screen.getByText("Compose · docker compose")).toBeTruthy();
  });
});
