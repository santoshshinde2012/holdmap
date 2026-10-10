import { expect, test } from "./fixtures";

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
});
