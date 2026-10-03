// @vitest-environment jsdom
// Behaviour and accessibility of the UI kit and the forms built on it.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import TextField from "./TextField.svelte";
import Switch from "./Switch.svelte";
import SegmentedControl from "./SegmentedControl.svelte";
import Select from "./Select.svelte";
import NumberInput from "./NumberInput.svelte";
import Tabs from "./Tabs.svelte";
import Checkbox from "./Checkbox.svelte";
import ConfirmDialog from "../ConfirmDialog.svelte";
import PinDialog from "../PinDialog.svelte";
import { MOCK_SNAPSHOT, mockPlan } from "../../lib/mock";

beforeAll(() => {
  // jsdom lacks the Web Animations API that Svelte transitions use.
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
  Element.prototype.scrollIntoView ??= () => {};
});
afterEach(cleanup);
const flush = () => new Promise((r) => setTimeout(r, 0));

describe("TextField", () => {
  it("links label, hint and error, and marks invalid", () => {
    render(TextField, { label: "Host", hint: "Uses ~/.ssh/config", error: "Bad host", value: "x" });
    const input = screen.getByLabelText("Host");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    const ids = input.getAttribute("aria-describedby")!.split(" ");
    const text = ids.map((id) => document.getElementById(id)?.textContent).join(" ");
    expect(text).toContain("Bad host");
  });
  it("clears with the button and with Esc", async () => {
    const onclear = vi.fn();
    render(TextField, { label: "Search", value: "next", clearable: true, onclear });
    const input = screen.getByLabelText("Search") as HTMLInputElement;
    await fireEvent.click(screen.getByLabelText("Clear search"));
    expect(input.value).toBe("");
    expect(onclear).toHaveBeenCalledOnce();
    await userEvent.type(input, "abc");
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("");
  });
  it("shows a character counter", () => {
    render(TextField, { label: "Label", value: "docs", maxlength: 40, counter: true });
    expect(screen.getByText("4/40")).toBeTruthy();
  });
});

describe("Switch and Checkbox", () => {
  it("switch toggles aria-checked and reports changes", async () => {
    const onchange = vi.fn();
    render(Switch, { label: "Launch at login", description: "Start in the menu bar", checked: false, onchange });
    const sw = screen.getByRole("switch", { name: "Launch at login" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    await fireEvent.click(sw);
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(onchange).toHaveBeenCalledWith(true);
    expect(document.getElementById(sw.getAttribute("aria-describedby")!)?.textContent).toContain("menu bar");
  });
  it("disabled switch does nothing", async () => {
    const onchange = vi.fn();
    render(Switch, { label: "Notify", disabled: true, onchange });
    await fireEvent.click(screen.getByRole("switch"));
    expect(onchange).not.toHaveBeenCalled();
  });
  it("checkbox is a labelled native control", async () => {
    const onchange = vi.fn();
    render(Checkbox, { label: "I understand", onchange });
    await fireEvent.click(screen.getByLabelText("I understand"));
    expect(onchange).toHaveBeenCalledWith(true);
  });
});

describe("SegmentedControl and Tabs", () => {
  const options = [{ value: "any", label: "Any" }, { value: "tcp", label: "TCP" }, { value: "udp", label: "UDP" }];
  it("segmented control is a radiogroup with roving arrow keys", async () => {
    const onchange = vi.fn();
    render(SegmentedControl, { label: "Protocol", value: "any", options, onchange });
    const radios = screen.getAllByRole("radio");
    expect(radios.map((r) => r.getAttribute("tabindex"))).toEqual(["0", "-1", "-1"]);
    radios[0].focus();
    await fireEvent.keyDown(radios[0], { key: "ArrowRight" });
    expect(onchange).toHaveBeenLastCalledWith("tcp");
    expect(radios[1].getAttribute("aria-checked")).toBe("true");
    await fireEvent.keyDown(radios[1], { key: "End" });
    expect(onchange).toHaveBeenLastCalledWith("udp");
  });
  it("tabs pair with panels and move with arrows", async () => {
    const onchange = vi.fn();
    render(Tabs, { base: "t", label: "Details", value: "a", tabs: [{ id: "a", label: "Overview" }, { id: "b", label: "Network", count: 2 }], onchange });
    const tabs = screen.getAllByRole("tab");
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
    expect(tabs[0].getAttribute("aria-controls")).toBe("t-panel-a");
    await fireEvent.keyDown(tabs[0], { key: "ArrowRight" });
    expect(onchange).toHaveBeenCalledWith("b");
    expect(tabs[1].getAttribute("aria-selected")).toBe("true");
  });
});

describe("Select", () => {
  const options = [{ value: "group", label: "Grouped" }, { value: "port", label: "Port" }, { value: "newest", label: "Newest" }];
  it("opens with the keyboard, moves, and commits with Enter", async () => {
    const onchange = vi.fn();
    render(Select, { label: "Sort by", value: "group", options, onchange });
    const box = screen.getByRole("combobox");
    expect(box.getAttribute("aria-expanded")).toBe("false");
    box.focus();
    await fireEvent.keyDown(box, { key: "ArrowDown" });
    await tick();
    expect(box.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getAllByRole("option")).toHaveLength(3);
    await fireEvent.keyDown(box, { key: "ArrowDown" });
    await fireEvent.keyDown(box, { key: "Enter" });
    await tick();
    expect(onchange).toHaveBeenCalledWith("port");
    expect(box.getAttribute("aria-expanded")).toBe("false");
  });
  it("Esc closes without changing the value", async () => {
    const onchange = vi.fn();
    render(Select, { label: "Sort by", value: "group", options, onchange });
    const box = screen.getByRole("combobox");
    await fireEvent.keyDown(box, { key: "ArrowDown" });
    await fireEvent.keyDown(box, { key: "ArrowDown" });
    await fireEvent.keyDown(box, { key: "Escape" });
    await tick();
    expect(box.getAttribute("aria-expanded")).toBe("false");
    expect(onchange).not.toHaveBeenCalled();
  });
});

describe("NumberInput", () => {
  it("steps with arrows and clamps to the range", async () => {
    const onchange = vi.fn();
    render(NumberInput, { label: "Scan every", value: 59, min: 1, max: 60, onchange });
    const sb = screen.getByRole("spinbutton");
    await fireEvent.keyDown(sb, { key: "ArrowUp" });
    expect(onchange).toHaveBeenLastCalledWith(60);
    await fireEvent.keyDown(sb, { key: "ArrowUp" });
    expect(onchange).toHaveBeenLastCalledWith(60);
    expect((screen.getByLabelText("Increase") as HTMLButtonElement).disabled).toBe(true);
  });
  it("explains out-of-range input after the field is touched", async () => {
    render(NumberInput, { label: "Port", value: 3000, min: 1, max: 65535 });
    const sb = screen.getByRole("spinbutton") as HTMLInputElement;
    await userEvent.clear(sb);
    await userEvent.type(sb, "70000");
    await fireEvent.blur(sb);
    expect(sb.getAttribute("aria-invalid")).toBe("true");
    expect(document.body.textContent).toMatch(/1.65535/);
  });
});

describe("Dialogs", () => {
  const entry = (port: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === port)!;

  it("stop-anyway requires the explicit checkbox", async () => {
    const onoverride = vi.fn();
    render(ConfirmDialog, { entry: entry(49152), plan: mockPlan("49152", false), phase: "confirm", log: [], report: null, onconfirm: vi.fn(), oncancel: vi.fn(), onoverride });
    const review = screen.getByRole("button", { name: /Review stop/ }) as HTMLButtonElement;
    expect(review.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText(/I understand/));
    expect(review.disabled).toBe(false);
    await fireEvent.click(review);
    expect(onoverride).toHaveBeenCalledOnce();
  });

  it("confirm is an alertdialog that closes on Esc and focuses the primary action", async () => {
    const oncancel = vi.fn();
    render(ConfirmDialog, { entry: entry(3000), plan: mockPlan("3000", false), phase: "confirm", log: [], report: null, onconfirm: vi.fn(), oncancel, onoverride: vi.fn() });
    const dlg = screen.getByRole("alertdialog");
    await flush();
    expect(document.activeElement?.textContent).toMatch(/Stop/);
    await fireEvent.keyDown(dlg, { key: "Escape" });
    expect(oncancel).toHaveBeenCalledOnce();
  });

  it("pin dialog validates, traps focus and saves", async () => {
    const onsave = vi.fn(async () => {});
    const onclose = vi.fn();
    render(PinDialog, { port: null, label: "", pinned: false, inUse: () => null, onsave, onunpin: vi.fn(), onclose });
    await flush();
    const port = document.getElementById("pin-port") as HTMLInputElement;
    expect(document.activeElement).toBe(port);
    await fireEvent.click(screen.getByRole("button", { name: /Pin port/ }));
    await tick();
    expect(port.getAttribute("aria-invalid")).toBe("true");
    expect(onsave).not.toHaveBeenCalled();

    // Shift+Tab from the first control wraps to the last one (focus trap).
    await userEvent.tab({ shift: true });
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);

    await userEvent.type(port, "8080");
    await userEvent.type(document.getElementById("pin-label")!, "api");
    await fireEvent.click(screen.getByRole("button", { name: /Pin port/ }));
    await flush();
    expect(onsave).toHaveBeenCalledWith(8080, "api");
    expect(onclose).toHaveBeenCalled();
  });
});
