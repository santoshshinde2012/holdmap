// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import SecurityEmpty from "./SecurityEmpty.svelte";

afterEach(cleanup);

describe("SecurityEmpty", () => {
  it("first run: says no analyzer is on and offers to choose one", async () => {
    const onprimary = vi.fn();
    const view = render(SecurityEmpty, { kind: "setup", onprimary });
    expect(view.getByText("No analyzer is on")).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Choose an analyzer" }));
    expect(onprimary).toHaveBeenCalledOnce();
  });
  it("all clear: reassures without any action", () => {
    const view = render(SecurityEmpty, { kind: "clear" });
    expect(view.getByText("No open findings")).toBeTruthy();
    expect(view.queryByRole("button")).toBeNull();
  });
  it("filters hide everything: offers to clear them", async () => {
    const onclear = vi.fn();
    const view = render(SecurityEmpty, { kind: "nomatch", onclear });
    expect(view.getByText("No findings match")).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Clear filters" }));
    expect(onclear).toHaveBeenCalledOnce();
  });
});
