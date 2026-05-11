/**
 * UI-driven signup — the only spec that exercises Clerk's interactive
 * SignUp component. Every other spec in this suite uses signInAs() to mount
 * a JWT and bypass the UI flow.
 *
 * Flow:
 *   1. Generate a unique `+clerk_test` alias under mailinator. Clerk's
 *      documented testing email pattern shortcuts OTP delivery in DEV
 *      instances with test mode enabled.
 *   2. Drive Clerk's SignUp form (email + password) and submit.
 *   3. Drive the OTP verification screen using Clerk's testing-helper code
 *      "424242" (https://clerk.com/docs/testing/test-emails-and-phones).
 *   4. Land on the authenticated dashboard.
 *   5. Cleanup: delete the freshly-created Clerk user so signup flows do
 *      not accumulate cruft in Clerk DEV.
 *
 * Prereq: Clerk DEV instance must have test mode enabled
 * (Configure → Email, Phone, Username → "Test mode" toggle on the email
 * field). Without it, "424242" is rejected and this spec hangs.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";

const PASSWORD = "SignupFixture!2026-stable";
const TEST_OTP = "424242";
const SIGNUP_TIMEOUT_MS = 30_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-fixture-${tag}+clerk_test@mailinator.com`;
}

test.describe("signup", () => {
  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) {
      await deleteUser(userId).catch(() => undefined);
    }
    createdEmail = null;
  });

  test("user signs up via UI and lands on the authenticated dashboard", async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;

    await page.goto("/sign-up");

    // Exact label match: Clerk renders a "Show password" toggle button next
    // to the input that also carries an aria-label containing "password", so
    // a loose /password/i match is a strict-mode violation.
    await page.getByLabel("Email address", { exact: true }).fill(email);
    await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await page.getByRole("button", { name: /continue|sign up/i }).first().click();

    // Clerk DEV always presents an email-verification step. Drive it using
    // the published testing OTP. Clerk renders six independent digit inputs
    // (an OTP-style segmented field) — type the code into the active first
    // box and Clerk's input handler distributes the digits across the
    // remaining boxes.
    const otpInput = page
      .getByRole("textbox", { name: /verification|code|enter/i })
      .first();
    await otpInput.waitFor({ timeout: SIGNUP_TIMEOUT_MS });
    await otpInput.fill(TEST_OTP);

    // Some Clerk SignUp variants auto-submit on the 6th digit; others wait
    // for an explicit Continue. Click Continue if it's present, otherwise
    // rely on the auto-submit.
    const continueBtn = page.getByRole("button", { name: /continue|verify/i });
    if (await continueBtn.first().isVisible().catch(() => false)) {
      await continueBtn.first().click();
    }

    await page.waitForURL(
      (url) => !url.toString().includes("/sign-up") && !url.toString().includes("/sign-in"),
      { timeout: SIGNUP_TIMEOUT_MS },
    );

    expect(page.url()).not.toContain("/sign-in");
    expect(page.url()).not.toContain("/sign-up");
    await expect(page.locator("body")).toContainText(/usezombie|Zombies|Dashboard/i);
  });
});
