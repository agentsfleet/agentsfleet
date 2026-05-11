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

// Skip signup against PROD: Clerk PROD almost certainly does not have
// test mode enabled (a DEV-only configuration), so the documented
// `+clerk_test@mailinator.com` alias would not short-circuit OTP and the
// spec would either hang on the verification screen or send a real OTP
// to a publicly-readable mailinator inbox. Both are unsafe.
const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.usezombie.com");

test.describe("signup", () => {
  test.skip(isProdApi, "signup spec only runs against DEV/local — see comment above");

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
    // remaining boxes. Selector: Clerk emits autocomplete="one-time-code"
    // on every segment input, which is stable across component versions
    // and survives the absence of an aria-label on the inputs themselves.
    const otpInput = page.locator('input[autocomplete="one-time-code"]').first();
    await otpInput.waitFor({ timeout: SIGNUP_TIMEOUT_MS });
    await otpInput.fill(TEST_OTP);

    // Some Clerk SignUp variants auto-submit on the 6th digit; others wait
    // for an explicit Continue. Click Continue if it's present, otherwise
    // rely on the auto-submit. Playwright's `isVisible()` returns a
    // boolean (never throws on a chained `.first()` locator), so no
    // catch fallback is needed.
    const continueBtn = page.getByRole("button", { name: /continue|verify/i });
    if (await continueBtn.first().isVisible()) {
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
