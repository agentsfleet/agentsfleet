import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAsUser } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { deleteUser, finalizeFixtureMetadata, provisionUser } from "./fixtures/clerk-admin";
import { bootstrapTenant } from "./fixtures/bootstrap";

const WIDTHS = [1440, 390, 320];
const ACCOUNT_LABEL = "Account";
const CANCEL_LABEL = "Cancel";
const FIXTURE_TAG_BYTES = 4;
const FIXTURE_PASSWORD_BYTES = 32;
let accountUserId: string | null = null;

test.beforeAll(async () => {
  // Security renders the account's active devices. A shared fixture makes
  // this accessibility workload grow with sessions from unrelated suite runs.
  const tag = crypto.randomBytes(FIXTURE_TAG_BYTES).toString("hex");
  const user = await provisionUser({
    key: "regular",
    email: `signup-fixture-${tag}+clerk_test@e2e.agentsfleet.net`,
    password: crypto.randomBytes(FIXTURE_PASSWORD_BYTES).toString("base64url"),
  });
  accountUserId = user.clerkUserId;
  await bootstrapTenant(user);
  await finalizeFixtureMetadata(user);
});

test.afterAll(async () => {
  if (accountUserId) await deleteUser(accountUserId);
});

for (const width of WIDTHS) {
  test(`account profile and security remain accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    if (!accountUserId) throw new Error("Account accessibility fixture was not provisioned");
    await signInAsUser(page, accountUserId);
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
