import { expect, openAgents, test } from "./fixtures";

test("List search and details navigate process and network sections", async ({ app }) => {
  const search = app.getByLabel("Search ports", { exact: true });
  await search.fill("3000");
  const row = app.getByRole("option", { name: /^Port 3000 tcp,/ });
  await expect(app.getByRole("listbox", { name: "Ports in use" }).getByRole("option")).toHaveCount(1);
  await row.click();
  await expect(row).toHaveAttribute("aria-selected", "true");
  const details = app.getByRole("complementary", { name: "Port details", exact: true });
  await expect(details.getByRole("heading", { name: /^shop-web/ })).toBeVisible();
  await details.getByRole("tab", { name: "Process", exact: true }).click();
  await expect(details.getByRole("tabpanel", { name: "Process", exact: true })).toContainText("43000");
  await details.getByRole("tab", { name: "Network", exact: true }).click();
  await expect(details.getByRole("tabpanel", { name: "Network", exact: true })).toContainText("127.0.0.1");
  await search.press("Escape");
  await expect(search).toHaveValue("");
  await expect(app.getByRole("listbox", { name: "Ports in use" }).getByRole("option")).not.toHaveCount(1);
});

test("Graph selection opens the corresponding listener and survives layout changes", async ({ app }) => {
  const graph = app.getByRole("radio", { name: /^Graph/ });
  await graph.click();
  await expect(graph).toHaveAttribute("aria-checked", "true");
  const service = app.getByRole("button", { name: "shop-api on ports 3001", exact: true });
  await expect(service).toBeVisible();
  const wrappedUsage = await app.locator(".node .usage > span").evaluateAll((spans) => spans
    .map((span) => ({ text: span.textContent, height: parseFloat(getComputedStyle(span).height), lineHeight: parseFloat(getComputedStyle(span).lineHeight) }))
    .filter((span) => span.height > span.lineHeight + 1));
  expect(wrappedUsage, "Graph resources should retain their units on one line").toEqual([]);
  await service.click();
  const details = app.getByRole("complementary", { name: "Port details", exact: true });
  await expect(details.getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  await app.getByRole("radio", { name: "Force", exact: true }).click();
  await expect(service).toBeVisible();
  await app.getByRole("radio", { name: "Layered", exact: true }).click();
  await expect(service).toBeVisible();
  await app.getByRole("radio", { name: "List", exact: true }).click();
  await expect(app.getByRole("option", { name: /^Port 3001 tcp,/ })).toHaveAttribute("aria-selected", "true");
});

test("Agents and Flow shortcuts leave hidden Ports filters unchanged", async ({ app }) => {
  await openAgents(app);
  const sendFilterKeys = async () => {
    await app.locator("body").click({ position: { x: 2, y: 2 } });
    for (const key of ["a", "t", "d", "m", "e", "p", "o", "c", "Delete"]) await app.keyboard.press(key);
  };
  await sendFilterKeys();
  await app.getByRole("radio", { name: "Flow", exact: true }).click();
  await sendFilterKeys();
  await expect(app.getByRole("dialog")).toHaveCount(0);
  await app.keyboard.press("Control+k");
  const palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("Only dev servers");
  await expect(palette.getByRole("option", { name: "Only dev servers" })).toHaveCount(0);
  await app.keyboard.press("Escape");
  await app.getByRole("radio", { name: "List", exact: true }).click();
  const filters = app.getByRole("toolbar", { name: "View and filters" });
  for (const name of ["Dev servers", "Mine", "Exposed"]) await expect(filters.getByRole("button", { name: new RegExp(`^${name}`) })).toHaveAttribute("aria-pressed", "false");
  await expect(app.getByRole("radio", { name: "Listening", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: "Any", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("Single-key shortcuts can be disabled and stay off after reload", async ({ app }) => {
  const list = app.getByRole("listbox", { name: "Ports in use" });
  const dev = app.getByRole("toolbar", { name: "View and filters" }).getByRole("button", { name: /^Dev servers/ });
  await list.focus();
  await app.keyboard.press("d");
  await expect(dev).toHaveAttribute("aria-pressed", "true");
  await app.keyboard.press("d");
  await app.keyboard.press("Control+,");
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await settings.getByRole("switch", { name: "Single-key shortcuts" }).click();
  await expect(settings.getByRole("switch", { name: "Single-key shortcuts" })).toHaveAttribute("aria-checked", "false");
  await settings.getByRole("button", { name: "Done", exact: true }).click();
  await app.reload();
  await expect(list.getByRole("option").first()).toBeVisible();
  await list.focus();
  for (const key of ["d", "/", "?"]) await app.keyboard.press(key);
  await expect(dev).toHaveAttribute("aria-pressed", "false");
  await expect(app.getByRole("dialog")).toHaveCount(0);
  await expect(app.getByLabel("Search ports", { exact: true })).not.toBeFocused();
  await app.keyboard.press("ArrowDown");
  await expect(list.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
  await app.keyboard.press("Control+f");
  await expect(app.getByLabel("Search ports", { exact: true })).toBeFocused();
  await app.keyboard.press("Control+k");
  await expect(app.getByRole("dialog", { name: "Command palette", exact: true }).getByRole("combobox", { name: "Search commands" })).toBeFocused();
});

test("IME, inherited editors and modified navigation do not trigger app actions", async ({ app }) => {
  const list = app.getByRole("listbox", { name: "Ports in use" });
  await list.focus();
  await app.keyboard.press("ArrowDown");
  const selectedId = await list.getAttribute("aria-activedescendant");
  for (const key of ["Alt+ArrowDown", "Control+Home", "Shift+ArrowDown"]) await app.keyboard.press(key);
  await expect(list).toHaveAttribute("aria-activedescendant", selectedId!);
  await list.dispatchEvent("keydown", { key: "d", isComposing: true });
  await list.dispatchEvent("keydown", { key: "d", keyCode: 229 });
  await app.evaluate(() => {
    const editor = document.createElement("div");
    editor.contentEditable = "true";
    const child = document.createElement("span");
    child.tabIndex = 0;
    editor.append(child);
    document.body.append(editor);
    child.focus();
    child.dispatchEvent(new KeyboardEvent("keydown", { key: "d", bubbles: true }));
    editor.remove();
  });
  await expect(app.getByRole("toolbar", { name: "View and filters" }).getByRole("button", { name: /^Dev servers/ })).toHaveAttribute("aria-pressed", "false");
  const search = app.getByLabel("Search ports", { exact: true });
  await search.fill("3000");
  await search.dispatchEvent("keydown", { key: "Escape", isComposing: true });
  await expect(search).toHaveValue("3000");
});

test.describe("compact windows", () => {
  test.use({ viewport: { width: 800, height: 900 } });

  test("List listener details use a dismissible drawer", async ({ app }) => {
    await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
    const drawer = app.getByRole("dialog", { name: "Details for port 3000", exact: true });
    await expect(drawer).toBeVisible();
    await expect(drawer.getByRole("heading", { name: /^shop-web/ })).toBeVisible();
    await drawer.getByRole("button", { name: "Close details", exact: true }).click();
    await expect(drawer).toHaveCount(0);
    await expect(app.getByRole("listbox", { name: "Ports in use" })).toBeVisible();
  });

  test("the filter toolbar scrolls without widening the document", async ({ app }) => {
    const dimensions = await app.getByRole("toolbar", { name: "View and filters" }).evaluate((toolbar) => ({
      document: document.documentElement.scrollWidth, viewport: innerWidth,
      toolbar: toolbar.scrollWidth, visibleToolbar: toolbar.clientWidth,
    }));
    expect(dimensions.document).toBeLessThanOrEqual(dimensions.viewport + 1);
    expect(dimensions.toolbar).toBeGreaterThan(dimensions.visibleToolbar);
    const sort = app.getByRole("combobox", { name: /^Sort:/ });
    await sort.focus();
    await expect(sort).toBeFocused();
    await sort.press("Enter");
    await expect(app.getByRole("listbox", { name: "Sort by", exact: true })).toBeVisible();
  });
});
