import { expect, test, type Locator, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const VIEWPORTS = [1440, 390];

async function expectActionInViewport(page: Page, action: Locator) {
  await action.scrollIntoViewIfNeeded();
  await expect(action).toBeInViewport();
  const bounds = await action.boundingBox();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  const account = page.getByRole("button", {name: /^Workspace and account menu(?::|$)/});
  await expect(account).toBeVisible();
  const menuBounds = await account.boundingBox();
  expect(menuBounds!.x + menuBounds!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  await expectNoPageOverflow(page);
}

test("Secrets exposes hidden values, access limits and reachable management at both widths", async ({ page }) => {
  const workspaceId = await getDefaultWorkspaceId("regular");
  const client = clientFor("regular");
  const name = `e2e-alignment-${crypto.randomUUID()}`;
  const endpoint = `/v1/workspaces/${workspaceId}/secrets`;
  await client.post(endpoint, { name, data: { purpose: "visual regression fixture" } });
  try {
    await signInAs(page, "regular");
    for (const width of VIEWPORTS) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/w/${workspaceId}/secrets`);
      const table = page.getByRole("table");
      await expect(table.getByRole("columnheader")).toHaveText(["Name", "Type", "Value", "Available to", ""]);
      const row = table.getByRole("row").filter({hasText: name});
      await expect(row.getByRole("cell").nth(2)).toHaveText("Hidden");
      await expect(row.getByRole("cell").nth(3)).toHaveText("Access details unavailable");
      const manage = row.getByRole("button", {name: `Manage ${name}`, exact: true});
      await expectActionInViewport(page, manage);
      await expectAccessible(page, "Populated secrets table");
      for (const action of ["Replace value", "Delete secret"]) {
        await manage.click();
        const management = page.getByRole("dialog");
        await expect(management).toContainText("Stored in this workspace");
        await expectAccessible(page, "Secret management");
        await management.getByRole("button", {name: action, exact: true}).click();
        const dialog = page.getByRole("dialog").or(page.getByRole("alertdialog"));
        await expect(dialog).toBeVisible();
        await expect(dialog).toHaveAccessibleName(/.+/);
        await expect(dialog).toHaveAccessibleDescription(/.+/);
        await expectAccessible(page, `${action} dialog`);
        await page.keyboard.press("Escape");
        await expect(dialog).toBeHidden();
        await expect(manage).toBeFocused();
      }
    }
  } finally {
    await client.delete(`${endpoint}/${encodeURIComponent(name)}`);
  }
});

test("Security key rows retain exact creation dates and reachable actions at both widths", async ({ page }) => {
  const client = clientFor("admin");
  const name = `e2e-alignment-${crypto.randomUUID()}`;
  const { id } = await client.post<{ id: string }>("/v1/api-keys", { key_name: name });
  const listing = await client.get<{items: {id: string; created_at: string}[]}>("/v1/api-keys");
  const createdAt = listing.items.find(key => key.id === id)?.created_at;
  expect(createdAt).toBeTruthy();
  try {
    await signInAs(page, "admin");
    for (const width of VIEWPORTS) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/settings/security");
      const row = page.getByRole("listitem").filter({hasText: name});
      await expect(row).toBeVisible();
      await expect(row.locator("time")).toHaveAttribute("datetime", new Date(createdAt!).toISOString());
      await expectActionInViewport(page, row.getByRole("button", {name: `Revoke API key ${name}`, exact: true}));
      await expectAccessible(page, "Security with API keys and sessions");
    }
  } finally {
    await client.patch(`/v1/api-keys/${id}`, { active: false });
    await client.delete(`/v1/api-keys/${id}`);
  }
});
