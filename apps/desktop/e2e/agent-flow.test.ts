import { expect, openAgents, test } from "./fixtures";

async function openFlow(app: Parameters<typeof openAgents>[0]) {
  await openAgents(app);
  await app.getByRole("radio", { name: "Flow", exact: true }).click();
  await expect(app.getByRole("region", { name: "Agent flow", exact: true })).toBeVisible();
}

test("Flow keeps observed socket evidence separate from reported calls", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await expect(flow.getByText("Example data", { exact: true })).toBeVisible();
  await expect(flow.getByRole("region", { name: "Flow participants", exact: true })).toBeVisible();
  await expect(flow.getByRole("region", { name: "Flow relationships", exact: true })).toBeVisible();
  await flow.getByRole("region", { name: "Flow relationships" }).getByRole("button").filter({ hasText: "live socket" }).first().click();
  const details = flow.getByRole("region", { name: "Selected relationship details" });
  await expect(details).toContainText("Observed source PIDs");
  await expect(details).toContainText("Direction, messages and transferred bytes cannot be determined");
  await expect(flow.locator(".svelte-flow__edge.animated")).toHaveCount(0);
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  await expect(flow.getByRole("region", { name: "Recorded activity timeline" })).toBeVisible();
  await expect(flow).toContainText("Event order is not proof of communication");
  await expect(flow.getByRole("button", { name: "Start recording", exact: true })).toBeDisabled();
});

test("reported call inspection and run selection stay stable during refresh and pause", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const run = flow.locator("select#agent-flow-run");
  await expect(run).toBeVisible();
  const value = await run.inputValue();
  await flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button").first().click();
  const details = flow.getByRole("region", { name: "Selected relationship details" });
  const name = await details.getByRole("heading").textContent();
  await expect(details).toContainText("Serialized input");
  await flow.getByRole("button", { name: "Refresh recorded activity" }).click();
  await expect(run).toHaveValue(value);
  await expect(details.getByRole("heading")).toHaveText(name!);
  await flow.getByRole("button", { name: "Pause flow updates" }).click();
  await expect(flow.getByRole("button", { name: "Refresh recorded activity" })).toBeEnabled();
  await expect(flow.getByText("View paused", { exact: true })).toBeVisible();
  await flow.getByRole("button", { name: "Refresh recorded activity" }).click();
  await expect(details.getByRole("heading")).toHaveText(name!);
  await flow.getByRole("button", { name: "Resume flow updates" }).click();
  await expect(flow.getByRole("button", { name: "Refresh recorded activity" })).toBeEnabled();
});

test("Flow has keyboard controls, a readable narrow layout and honest setup guidance", async ({ app }) => {
  await app.setViewportSize({ width: 820, height: 800 });
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await expect(flow.getByRole("button", { name: "Fit all relationships in view" })).toBeVisible();
  await app.getByRole("radio", { name: "Connections", exact: true }).focus();
  await app.keyboard.press("ArrowRight");
  await expect(app.getByRole("radio", { name: "Calls", exact: true })).toHaveAttribute("aria-checked", "true");
  await flow.getByRole("button", { name: "Setup", exact: true }).click();
  await expect(flow.getByRole("region", { name: "Recording setup" })).toContainText("Recording is available in the local desktop app");
  await expect(flow).toContainText("Settings are never changed automatically");
  await expect(flow.getByRole("button", { name: /Copy .* settings/ })).toHaveCount(0);
  await flow.getByRole("button", { name: "Setup", exact: true }).click();
  const run = flow.locator("#agent-flow-run");
  const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
  await run.selectOption(trace!);
  await expect.poll(async () => (await flow.locator(".flow-node").first().boundingBox())?.width ?? 0).toBeGreaterThan(145);
  await flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button").filter({ hasText: "Tool execution" }).click();
  await expect(flow.getByRole("region", { name: "Selected relationship details" }).getByRole("heading", { name: "Tool execution", exact: true })).toBeVisible();
  const canvas = await flow.locator(".canvas").boundingBox();
  expect(canvas!.y).toBeGreaterThan(0); expect(canvas!.y + canvas!.height).toBeLessThan(800);
  expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("full recorded trace shows parallel tools, a failed step and a visible inspector", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const run = flow.locator("#agent-flow-run");
  const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
  await run.selectOption(trace!);
  await expect(flow.locator(".flow-node")).toHaveCount(10);
  await expect(flow.locator(".svelte-flow__edge")).toHaveCount(9);
  await expect(flow).toContainText("1 failed span");
  await flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button").filter({ hasText: "Tool execution" }).click();
  const details = flow.getByRole("region", { name: "Selected relationship details" });
  await expect(details.getByRole("heading", { name: "Tool execution", exact: true })).toBeVisible();
  await expect(details).toContainText("Failed");
  await expect(flow.locator(".flow-node.failed")).toHaveCount(1);
  await flow.getByRole("button", { name: "Close relationship details" }).click();
  await expect(details).toHaveCount(0);
});

test("Flow keyboard controls cannot act on a previously selected listener", async ({ app }) => {
  await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const record = flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button").first();
  await record.click();
  await record.press("Delete");
  await expect(app.getByRole("alertdialog")).toHaveCount(0);
  await record.press("Backspace");
  await expect(app.getByRole("alertdialog")).toHaveCount(0);
  await expect(flow.getByRole("region", { name: "Selected relationship details" })).toBeVisible();
});

test("the run selector retains its native arrow-key behavior", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const run = flow.locator("#agent-flow-run");
  await expect(run.locator("option")).toHaveCount(2);
  const target = await run.locator("option").nth(1).getAttribute("value");
  await app.evaluate(() => {
    const picker = document.querySelector<HTMLSelectElement>("#agent-flow-run")!;
    window.addEventListener("keydown", (event) => {
      if (document.activeElement === picker) picker.dataset.keyPrevented = String(event.defaultPrevented);
    });
  });
  await run.focus();
  await run.press("ArrowDown");
  // Native popup selection differs across operating systems; App must not cancel it.
  await expect(run).toHaveAttribute("data-key-prevented", "false");
  await run.press("Escape");
  await run.selectOption(target!);
  await expect(run).toHaveValue(target!);
  await expect(flow.locator(".flow-node")).toHaveCount(10);
});

test("Calls filters isolate failures and missing completions while preserving reported context", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const run = flow.locator("#agent-flow-run");
  const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
  await run.selectOption(trace!);
  const timeline = flow.getByRole("region", { name: "Recorded activity timeline" });
  await flow.getByRole("radio", { name: "Failed", exact: true }).click();
  await expect(flow.getByText("1 of 10 records shown", { exact: true })).toBeVisible();
  await expect(timeline.getByRole("button")).toHaveCount(1);
  await expect(timeline).toContainText("Tool execution");
  await expect(flow.locator(".flow-node")).toHaveCount(5);
  await expect(flow.locator(".svelte-flow__edge")).toHaveCount(4);
  await timeline.getByRole("button").click();
  const inspector = flow.getByRole("region", { name: "Selected relationship details" });
  await flow.getByRole("radio", { name: "No completion", exact: true }).click();
  await expect(flow.getByText("0 of 10 records shown", { exact: true })).toBeVisible();
  await expect(timeline.getByRole("button")).toHaveCount(0);
  await expect(inspector.getByRole("heading", { name: "Tool execution", exact: true })).toBeVisible();
  await flow.getByRole("button", { name: "Close relationship details" }).click();
  await expect(flow.locator(".flow-node")).toHaveCount(0);
  await expect(flow.getByRole("heading", { name: "No activity matches this view", exact: true })).toBeVisible();
  await flow.getByRole("button", { name: "Reset activity view", exact: true }).click();
  await expect(flow.getByRole("button", { name: "Reset activity view", exact: true })).toBeFocused();
  await expect(timeline.getByRole("button")).toHaveCount(10);
  await expect(run).toHaveValue(trace!);
  const pending = await run.locator("option").filter({ hasText: "mcp__browser__open" }).getAttribute("value");
  await run.selectOption(pending!);
  await flow.getByRole("radio", { name: "All", exact: true }).focus();
  await app.keyboard.press("End");
  await expect(flow.getByRole("radio", { name: "No completion", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(flow.getByText("1 of 1 records shown", { exact: true })).toBeVisible();
  await expect(timeline).toContainText("No completion received");
});

test("Calls metadata search and native duration ordering retain the current run", async ({ app }) => {
  await app.setViewportSize({ width: 820, height: 800 });
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await app.getByRole("radio", { name: "Calls", exact: true }).click();
  const run = flow.locator("#agent-flow-run");
  const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
  await run.selectOption(trace!);
  const timeline = flow.getByRole("region", { name: "Recorded activity timeline" });
  const order = flow.getByRole("combobox", { name: "Order", exact: true });
  await order.selectOption("duration");
  await expect(timeline.getByRole("button").nth(1)).toContainText("Delegate to reviewer");
  await app.getByLabel("Search agents", { exact: true }).fill("Tool execution");
  await expect(run).toHaveValue(trace!);
  await expect(timeline.getByRole("button")).toHaveCount(1);
  await expect(flow.locator(".flow-node")).toHaveCount(5);
  await flow.getByRole("button", { name: "Reset activity view", exact: true }).click();
  await expect(app.getByLabel("Search agents", { exact: true })).toHaveValue("");
  await expect(order).toHaveValue("time");
  await expect(timeline.getByRole("button")).toHaveCount(10);
  await expect(timeline.getByRole("button").nth(1)).toContainText("Model request");
  expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
