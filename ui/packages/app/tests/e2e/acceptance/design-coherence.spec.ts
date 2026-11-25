import { expect, test, type Locator, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const VIEWPORTS = [1440, 390];
// Type, Value and Available to are `hideOnMobile` columns, shown from Tailwind's
// `sm` width (40rem); below it the row's Manage dialog carries them.
const TABLE_COLUMNS_MIN_WIDTH = 640;
const WIDE_SECRET_COLUMNS = ["Name", "Type", "Value", "Available to", "Actions"];
const NARROW_SECRET_COLUMNS = ["Name", "Actions"];

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
      const wide = width >= TABLE_COLUMNS_MIN_WIDTH;
      await expect(table.getByRole("columnheader")).toHaveText(wide ? WIDE_SECRET_COLUMNS : NARROW_SECRET_COLUMNS);
      const row = table.getByRole("row").filter({hasText: name});
      if (wide) {
        await expect(row.getByRole("cell").nth(2)).toHaveText("Hidden");
        // A fixture secret no fleet declares is a recorded "none", never an unknown.
        await expect(row.getByRole("cell").nth(3)).toHaveText("No access approved");
      }
      const manage = row.getByRole("button", {name: `Manage ${name}`, exact: true});
      await expectActionInViewport(page, manage);
      await expectAccessible(page, "Populated secrets table");
      for (const action of ["Replace value", "Delete secret"]) {
        await manage.click();
        const management = page.getByRole("dialog");
        await expect(management).toContainText("Stored in this workspace");
        // At every width the dialog says the value is hidden and who may use it.
        await expect(management).toContainText("Value hidden");
        await expect(management).toContainText("No active fleet");
        await expectAccessible(page, "Secret management");
        await management.getByRole("button", {name: action, exact: true}).click();
        // Both dialogs keep a fixed title and name the secret in their description.
        const dialog = action === "Replace value"
          ? page.getByRole("dialog", {name: "Replace secret", exact: true})
          : page.getByRole("alertdialog", {name: "Delete secret?", exact: true});
        await expect(dialog).toBeVisible();
        await expect(dialog).toContainText(`“${name}”`);
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
  try {
    const listing = await client.get<{items: {id: string; created_at: string}[]}>("/v1/api-keys");
    const createdAt = listing.items.find(key => key.id === id)?.created_at;
    expect(createdAt).toBeTruthy();
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
