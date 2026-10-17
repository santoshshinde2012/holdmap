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
    // exposed + (+N) for container, links, and the memory badge on this 300 MB fixture
    expect(chips[1].textContent).toBe("+3");
    expect(chips[1].getAttribute("aria-label")).toMatch(/^3 more: Docker · 2 connected/);
  });

  it("actions overlay the row (absolutely positioned, not a grid column) and don't select the row", async () => {
    const h = handlers();
    const { container } = render(PortRow, { entry: byPort(3000), pinned: true, selected: false, busy: false, ...h });
    const actions = container.querySelector(".actions")!;
    expect(actions.parentElement!.classList.contains("row")).toBe(true);
    const act = (name: string) => actions.querySelector<HTMLElement>(`[data-action="${name}"]`)!;
    await fireEvent.click(act("stop"));
    await fireEvent.click(act("pin"));
    await fireEvent.click(act("open"));
    expect(h.onstop).toHaveBeenCalledOnce();
    expect(h.onpin).toHaveBeenCalledOnce();
    expect(h.onopen).toHaveBeenCalledOnce();
    expect(h.onselect).not.toHaveBeenCalled();
  });

  it("nests no controls in the option: actions are unfocusable and hidden from assistive tech", () => {
    const { container } = render(PortRow, { entry: byPort(3000), pinned: true, selected: true, busy: false, ...handlers() });
    const option = screen.getByRole("option");
    expect(option.querySelectorAll("button, a[href], input, select, textarea, [tabindex]")).toHaveLength(0);
    expect(container.querySelector(".actions")!.getAttribute("aria-hidden")).toBe("true");
    expect(screen.queryAllByRole("button")).toHaveLength(0);
  });

  it("hides Stop for protected ports and disables it while stopping", async () => {
    const h = handlers();
    const { container } = render(PortRow, { entry: byPort(5000), selected: false, busy: false, ...h });
    expect(container.querySelector('[data-action="stop"]')).toBeNull();
    cleanup();
    const busy = render(PortRow, { entry: byPort(3001), selected: false, busy: true, ...h });
    const stop = busy.container.querySelector<HTMLElement>('[data-action="stop"]')!;
    expect(stop.classList.contains("disabled")).toBe(true);
    await fireEvent.click(stop);
    expect(h.onstop).not.toHaveBeenCalled();
    expect(screen.getByRole("img", { name: "Stopping…" })).toBeTruthy();
  });

  it("marks the density on the row", () => {
    const { container } = render(PortRow, { entry: byPort(3001), density: "compact", selected: false, busy: false, ...handlers() });
    expect(container.querySelector(".row")!.classList.contains("compact")).toBe(true);
  });
});

describe("GroupHeader", () => {
  it("is a pointer affordance inside the listbox: names its group with count and state, no nested control", async () => {
    const ontoggle = vi.fn();
    const { container, rerender } = render(GroupHeader, { id: "dev", title: "Dev servers", count: 4, hint: "Your running projects", ontoggle });
    expect(screen.queryAllByRole("button", { hidden: true })).toHaveLength(0);
    expect(container.querySelector(".gh")!.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector("#gl-dev")!.textContent).toBe("Dev servers, 4 ports");
    expect(screen.queryByText("Your running projects")).toBeNull(); // generic blurb is a tooltip, not clutter
    await fireEvent.click(container.querySelector(".toggle")!);
    expect(ontoggle).toHaveBeenCalledOnce();
    await rerender({ id: "dev", title: "Dev servers", count: 4, collapsed: true, ontoggle });
    expect(container.querySelector("#gl-dev")!.textContent).toBe("Dev servers, 4 ports, collapsed");
  });
  it("shows cluster detail inline", () => {
    render(GroupHeader, { id: "c1", title: "shop", count: 3, hint: "Compose · docker compose", showHint: true, ontoggle: () => {} });
    expect(screen.getByText("Compose · docker compose")).toBeTruthy();
  });
});
