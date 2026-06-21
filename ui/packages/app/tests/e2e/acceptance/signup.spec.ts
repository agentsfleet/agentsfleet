/**
 * Signup smoke — exercises Clerk's browser-side signup path. Every other
 * spec in this suite uses signInAs() to mount a JSON Web Token (JWT) and
 * bypass user creation.
 *
 * Flow:
 *   1. Generate a unique `+clerk_test` alias under mailinator. Clerk's
 *      documented testing email pattern shortcuts one-time-code delivery in
 *      development instances with test mode enabled.
 *   2. Drive Clerk's browser signup helper and replay the identity event so
 *      agentsfleetd creates the tenant workspace.
 *   3. Land on the authenticated dashboard.
 *   4. Cleanup: delete the freshly-created Clerk user so signup flows do
 *      not accumulate cruft in Clerk development.
 *
 * The hosted SignUp component is not the stable assertion surface here: the
 * current Clerk development instance gates the form before the one-time-code
 * screen. fixtures/signup.ts owns the direct browser helper and keeps the
 * resulting session shape equivalent to a real signup.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { signUpAs } from "./fixtures/signup";

const PASSWORD = "SignupFixture!2026-stable";
const SIGNUP_TIMEOUT_MS = 30_000;
const SIGNUP_TEST_TIMEOUT_MS = 90_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-fixture-${tag}+clerk_test@mailinator.com`;
}

// Skip signup against production: Clerk production almost certainly does
// not have test mode enabled (a development-only configuration), so the
// documented `+clerk_test@mailinator.com` alias would not short-circuit
// one-time password (OTP) delivery and the spec would either hang on the
// verification screen or send a real OTP to a publicly-readable mailinator
// inbox. Both are unsafe.
const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("signup", () => {
  test.skip(isProdApi, "signup spec only runs against development/local — see comment above");
  test.setTimeout(SIGNUP_TEST_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) {
      await deleteUser(userId).catch(() => undefined);
    }
    createdEmail = null;
  });

  test("user signs up and lands on the authenticated dashboard", async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;

    await signUpAs(page, email, PASSWORD);
    await page.goto("/");

    await page.waitForURL(
      (url) => !url.toString().includes("/sign-up") && !url.toString().includes("/sign-in"),
      { timeout: SIGNUP_TIMEOUT_MS },
    );

    expect(page.url()).not.toContain("/sign-in");
    expect(page.url()).not.toContain("/sign-up");
    await expect(page.locator("body")).toContainText(/agentsfleet|Fleets|Dashboard/i);
  });
});
