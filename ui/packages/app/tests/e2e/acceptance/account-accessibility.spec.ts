import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAsUser } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { deleteUser, finalizeFixtureMetadata, provisionUser } from "./fixtures/clerk-admin";
import {captureAccountState} from "./fixtures/account-visual";
import { bootstrapTenant } from "./fixtures/bootstrap";

const WIDTHS = [1480, 390, 320];
const ACCOUNT_LABEL = "Security";
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
    await expect(page.getByRole("region", {name: "Your account", exact: true})).toHaveCount(0);
    await expect(page.getByRole("button", {name: "Profile and sign-in settings", exact: true})).toHaveCount(0);
    await expect(page.locator(".cl-userProfile-root")).toHaveCount(0);
    await captureAccountState(page, test.info(), "account-identity", width, theme);
    const sessions = page.getByRole("region", {name: "Active sessions", exact: true});
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await expect(page.getByRole("heading", {name: "Security", exact: true})).toBeVisible();
    await expectAccessible(page, "Security");
    await captureAccountState(page, test.info(), "keys-and-sessions", width, theme);
    await expectNoPageOverflow(page);

    await page.screenshot({path: test.info().outputPath(`live-security-${width}.png`), scale: "css", animations: "disabled"});
    await opener.click();
    await page.getByRole("menuitem", {name: ACCOUNT_LABEL, exact: true}).click();
    await expect(page).toHaveURL(/\/settings\/security/);
    await expect(page.getByText("Signed in as", {exact: true})).toHaveCount(0);
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await page.reload();
    await expect(page).toHaveURL(/\/settings\/security(?:#.*)?$/);
    await expect(sessions.getByText("This device", {exact: true})).toBeVisible();
    await expectAccessible(page, "Security deep link after reload");
    await expectNoPageOverflow(page);
  });
}
