import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAsUser } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { deleteUser, finalizeFixtureMetadata, provisionUser } from "./fixtures/clerk-admin";
import {captureAccountState} from "./fixtures/account-visual";
import { bootstrapTenant } from "./fixtures/bootstrap";

const WIDTHS = [1480, 390, 320];
const ACCOUNT_LABEL = "API Keys";
const CANCEL_LABEL = "Cancel";
const FIXTURE_TAG_BYTES = 4;
const FIXTURE_PASSWORD_BYTES = 32;
let accountUserId: string | null = null;

test.beforeAll(async () => {
  // Security renders the account's browser sessions. A shared fixture makes
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

for (const width of WIDTHS) for (const theme of ["dark", "light"]) {
  test(`account profile and security remain accessible at ${width}px in ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    if (!accountUserId) throw new Error("Account accessibility fixture was not provisioned");
    await signInAsUser(page, accountUserId);
    await page.goto("/");
    await page.evaluate(value => localStorage.setItem("theme", value), theme);
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await expect(page.getByRole("heading", {name: "What should your fleet do?", exact: true})).toBeVisible();
    const opener = page.getByRole("button", { name: /^Workspace and account menu(?::|$)/ });
    await expect(opener).toBeVisible();
    await opener.focus();
    await page.keyboard.press("Enter");
    const account = page.getByRole("menuitem", {name: ACCOUNT_LABEL, exact: true});
    await expect(account).toBeVisible();
    await expectAccessible(page, "Account menu");
    await account.click();
    await expect(page).toHaveURL(/\/settings\/security/);
    await page.getByRole("button", {name: "Profile and sign-in settings", exact: true}).click();
    await expect(page.locator(".cl-userProfile-root")).toBeVisible();
    const nativeAccount = page.getByRole("region", {name: "Profile and sign-in settings", exact: true});
    const nativeViewport = nativeAccount.locator(".cl-pageScrollBox");
    await expect(nativeViewport).toBeVisible();
    await nativeAccount.getByRole("button", {name: "Update profile", exact: true}).click();
    const firstName = nativeAccount.getByRole("textbox", {name: "First name", exact: true});
    await expect(firstName).toBeVisible();
    await captureAccountState(page, test.info(), "profile-edit", width, theme);
    await expect(nativeAccount.getByRole("textbox", {name: "Last name", exact: true})).toBeVisible();
    const original = await firstName.inputValue();
    await firstName.fill("Accessibility draft");
    await expectAccessible(page, "Native profile editing");
    await expectNoPageOverflow(page);
    await expect.poll(() => nativeViewport.evaluate(node => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
    await page.screenshot({path: test.info().outputPath(`live-profile-${width}.png`), scale: "css", animations: "disabled"});
    await nativeAccount.getByRole("button", {name: CANCEL_LABEL, exact: true}).click();
    await expect(firstName).toBeHidden();
    await nativeAccount.getByRole("button", {name: "Update profile", exact: true}).click();
    await expect(firstName).toHaveValue(original);
    await nativeAccount.getByRole("button", {name: CANCEL_LABEL, exact: true}).click();
    await page.getByRole("heading", {name: "API Keys", level: 1, exact: true}).scrollIntoViewIfNeeded();
    await expect(page).toHaveURL(/\/settings\/security(?:#.*)?$/);
    const sessions = page.getByRole("region", {name: "Active sessions", exact: true});
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await expect(page.getByRole("heading", {name: "API Keys", exact: true})).toBeVisible();
    await expectAccessible(page, "API Keys");
    await captureAccountState(page, test.info(), "keys-and-sessions", width, theme);
    await expectNoPageOverflow(page);

    await page.screenshot({path: test.info().outputPath(`live-security-${width}.png`), scale: "css", animations: "disabled"});
    await opener.click();
    await page.getByRole("menuitem", {name: ACCOUNT_LABEL, exact: true}).click();
    await expect(page).toHaveURL(/\/settings\/security/);
    await page.getByRole("button", {name: "Profile and sign-in settings", exact: true}).click();
    await expect(nativeAccount).toBeVisible();
    await expect(nativeViewport).toBeVisible();
    await expect.poll(() => nativeViewport.evaluate(node => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
    const security = nativeAccount.getByRole("button", {name: "Security", exact: true});
    if (width < 768) await nativeAccount.getByRole("button", {name: "Account", exact: true}).click();
    await security.click();
    await nativeAccount.getByRole("button", {name: "Update password", exact: true}).click();
    const password = nativeAccount.getByLabel("New password", {exact: true});
    await expect(password).toBeVisible();
    await expect(password).toBeEnabled();
    await captureAccountState(page, test.info(), "password-edit", width, theme);
    await expectAccessible(page, "Password editing");
    await expect.poll(() => nativeViewport.evaluate(node => node.scrollWidth <= node.clientWidth + 1)).toBe(true);
    const bounds = await nativeAccount.boundingBox();
    const field = await password.boundingBox();
    expect(bounds).not.toBeNull();
    expect(field).not.toBeNull();
    if (!bounds || !field) throw new Error("Native account controls must have measurable bounds");
    expect(field.x).toBeGreaterThanOrEqual(bounds.x - 1);
    expect(field.x + field.width).toBeLessThanOrEqual(bounds.x + bounds.width + 1);
    await page.screenshot({path: test.info().outputPath(`live-password-editor-${width}.png`), scale: "css", animations: "disabled"});
    await nativeAccount.getByRole("button", {name: CANCEL_LABEL, exact: true}).click();
    await expect(password).toBeHidden();
    await expect(nativeAccount.getByRole("button", {name: "Update password", exact: true})).toBeVisible();
    await page.getByRole("heading", {name: "API Keys", level: 1, exact: true}).scrollIntoViewIfNeeded();
    await expect(nativeAccount).toBeVisible();
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await page.reload();
    await expect(page).toHaveURL(/\/settings\/security(?:#.*)?$/);
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await expectAccessible(page, "Security deep link after reload");
    await expectNoPageOverflow(page);
  });
}
