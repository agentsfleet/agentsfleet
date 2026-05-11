/**
 * UI-driven signup — the only spec that exercises Clerk's interactive
 * SignUp component. Every other spec in this suite uses signInAs() to mount
 * a JWT and bypass the UI flow.
 *
 * Flow:
 *   1. Generate a unique email under the +clerk_test alias on mailinator.
 *      `+clerk_test` short-circuits Clerk's OTP delivery in DEV instances
 *      with test-mode enabled — no real email is sent or read.
 *   2. Drive Clerk's SignUp form (email + password) and land on the
 *      authenticated dashboard.
 *   3. Assert dashboard renders authenticated content (signed-in marker).
 *   4. Cleanup: delete the freshly-created Clerk user so signup flows do
 *      not accumulate cruft in Clerk DEV.
 *
 * Prereq: Clerk DEV instance must have test mode enabled
 * (Configure → Email, Phone, Username → "Test mode" toggle on the email
 * field). Without it, the +clerk_test alias does not bypass OTP and this
 * spec hangs at the verification step.
 *
 * Selectors below are educated guesses against Clerk's stock SignUp
 * component. First local run may need selector tuning if the rendered DOM
 * differs; the failure modes will surface in playwright-auth-report HTML.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";

const PASSWORD = "SignupFixture!2026-stable";

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

  // FIXME: Clerk DEV is showing an email-verification step the spec does
  // not drive. Two roads to unblock:
  //   1. Enable test mode on the Clerk DEV instance so the
  //      `signup+clerk_test@mailinator.com` alias bypasses the OTP loop.
  //   2. Drive the verification screen explicitly (parse OTP from
  //      mailinator or use Clerk's testing-helper OTP code "424242").
  // Tracked in M64_006 alongside the lifecycle/kill client-token issue.
  test.fixme("user signs up via UI and lands on the authenticated dashboard", async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;

    await page.goto("/sign-up");

    // Exact label match: Clerk renders a "Show password" toggle button next to
    // the input that also carries an aria-label containing "password", so a
    // loose /password/i match is a strict-mode violation.
    await page.getByLabel("Email address", { exact: true }).fill(email);
    await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await page.getByRole("button", { name: /continue|sign up/i }).first().click();

    await page.waitForURL((url) => !url.toString().includes("/sign-up"), { timeout: 30_000 });

    expect(page.url()).not.toContain("/sign-in");
    expect(page.url()).not.toContain("/sign-up");
    await expect(page.locator("body")).toContainText(/usezombie|Zombies|Dashboard/i);
  });
});
