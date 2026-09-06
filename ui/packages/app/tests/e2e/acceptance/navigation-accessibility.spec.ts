import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const WIDTHS = [1440, 390, 320];
const CREATE_WORKSPACE = "Create workspace";

for (const width of WIDTHS) {
  test(`workspace menu and creation preserve accessibility and keyboard focus at ${width}px`, async ({ page }) => {
    const workspaceId = await getDefaultWorkspaceId("regular");
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "regular");
    await page.goto(`/w/${workspaceId}/fleets`);
    await expect(page.getByRole("button", { name: "Open user menu" })).toBeVisible();
    const opener = page.getByTestId("workspace-switcher");
    await opener.focus();
    await page.keyboard.press("Enter");
    const create = page.getByRole("menuitem", { name: CREATE_WORKSPACE });
    await expect(create).toBeVisible();
    await expectAccessible(page, "Workspace menu");
    await expectNoPageOverflow(page);
    await page.keyboard.press("Escape");
    await expect(create).toBeHidden();
    await expect(opener).toBeFocused();
    await page.keyboard.press("Enter");
    await create.click();
    const dialog = page.getByRole("dialog", { name: CREATE_WORKSPACE });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAccessibleDescription(/.+/);
    await expect(page.getByRole("textbox", { name: "Name (optional)" })).toHaveAccessibleDescription(/.+/);
    await expectAccessible(page, "Create workspace");
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(opener).toBeFocused();
  });
}

for (const width of [390, 320]) {
  test(`mobile navigation exposes destinations and restores keyboard focus at ${width}px`, async ({ page }) => {
    const workspaceId = await getDefaultWorkspaceId("regular");
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "regular");
    await page.goto(`/w/${workspaceId}/fleets`);
    const opener = page.getByRole("button", { name: "Open navigation" });
    await opener.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("link", { name: "Secrets", exact: true })).toBeVisible();
    await expectAccessible(page, "Mobile navigation");
    await expectNoPageOverflow(page);
    await page.keyboard.press("Escape");
    await expect(opener).toBeFocused();
  });
}
