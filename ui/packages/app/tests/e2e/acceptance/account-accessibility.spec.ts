import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const WIDTHS = [1440, 390, 320];
const ACCOUNT_LABEL = "Account";
const CANCEL_LABEL = "Cancel";

for (const width of WIDTHS) {
  test(`account profile and security remain accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "regular");
    await page.goto("/");
    const opener = page.getByRole("button", { name: "Open user menu" });
    await expect(opener).toBeVisible();
    const avatarBox = await opener.boundingBox();
    const workspaceBox = await page.getByTestId("workspace-switcher").boundingBox();
    expect(avatarBox).not.toBeNull();
    expect(workspaceBox).not.toBeNull();
    expect(Math.abs((avatarBox!.y + avatarBox!.height / 2) - (workspaceBox!.y + workspaceBox!.height / 2))).toBeLessThanOrEqual(1);
    await opener.focus();
    await page.keyboard.press("Enter");
    const account = page.getByRole("button", { name: ACCOUNT_LABEL, exact: true });
    await expect(account).toBeVisible();
    await expectAccessible(page, "Account menu");
    await account.click();
    await expect(page.getByRole("button", { name: "Update profile" })).toBeVisible();
    await expect(page).toHaveURL(/\/settings\/account/);
    await expect(page.getByRole("heading", { name: "Account settings", exact: true })).toBeVisible();
    await expectAccessible(page, "Account profile");
    await expectNoPageOverflow(page);
    await page.getByRole("button", { name: "Update profile" }).click();
    await expect(page.getByRole("textbox", { name: "First name" })).toBeVisible();
    await expectAccessible(page, "Profile editing");
    await page.getByRole("button", { name: CANCEL_LABEL, exact: true }).click();
    if (width < 768) await page.getByRole("button", { name: ACCOUNT_LABEL, exact: true }).click();
    await page.getByRole("button", { name: "Security", exact: true }).click();
    await expect(page.getByText("Active devices", { exact: true })).toBeVisible();
    await expectAccessible(page, "Security");
    await page.getByRole("button", { name: "Update password" }).click();
    await expect(page.getByLabel("New password", { exact: true })).toBeVisible();
    await expectAccessible(page, "Password editing");
    await page.getByRole("button", { name: CANCEL_LABEL, exact: true }).click();
    await expect(page.getByText("Active devices", { exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByText("Active devices", { exact: true })).toBeVisible();
    await expectAccessible(page, "Security deep link after reload");
  });
}
