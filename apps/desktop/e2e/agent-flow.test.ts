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
  await flow.getByRole("radio", { name: "Calls", exact: true }).click();
  await expect(flow.getByRole("region", { name: "Recorded activity timeline" })).toBeVisible();
  await expect(flow).toContainText("Event order is not proof of communication");
  await expect(flow.getByRole("button", { name: "Start recording", exact: true })).toBeDisabled();
});

test("reported call inspection and run selection stay stable during refresh and pause", async ({ app }) => {
  await openFlow(app);
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await flow.getByRole("radio", { name: "Calls", exact: true }).click();
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
  await flow.getByRole("radio", { name: "Connections", exact: true }).focus();
  await app.keyboard.press("ArrowRight");
  await expect(flow.getByRole("radio", { name: "Calls", exact: true })).toHaveAttribute("aria-checked", "true");
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
  await flow.getByRole("radio", { name: "Calls", exact: true }).click();
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
  await flow.getByRole("radio", { name: "Calls", exact: true }).click();
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
  await flow.getByRole("radio", { name: "Calls", exact: true }).click();
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
