/**
 * Full-lifecycle Scenario 1 — ephemeral signup → workspace auto-provision →
 * install via dashboard UI → observe → bill → halt.
 *
 * End-to-end UI-driven: the operator never leaves the browser. signup goes
 * through Clerk's hosted form, install goes through /zombies/new, every
 * lifecycle transition is a real click in the dashboard. No API short-cuts.
 *
 * DEV-only — Clerk PROD almost certainly does not have test mode enabled, so
 * the `+clerk_test@mailinator.com` alias would not short-circuit OTP. Mirrors
 * the same isProdApi guard signup.spec.ts uses.
 *
 * Why this spec exists: the persistent-fixture suite covers each lifecycle
 * slice individually but no spec walks a fresh operator from "I just signed
 * up" through to "I just killed a zombie I observed running." The dashboard's
 * route-guard chain (workspace auto-provision, starter credit, empty-state →
 * populated transition) only runs on a brand-new tenant — that surface is
 * uncovered without this spec.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { installViaUI } from "./fixtures/install-ui";
import {
  expectDetailKilled,
  expectRowState,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle";

const PASSWORD = "SignupFixture!2026-stable";
const TEST_OTP = "424242";
const SIGNUP_TIMEOUT_MS = 30_000;
const FLOW_TIMEOUT_MS = 120_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-lifecycle-${tag}+clerk_test@mailinator.com`;
}

function uniqueName(): string {
  return `lifecycle-${crypto.randomBytes(4).toString("hex")}`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.usezombie.com");

test.describe("signup → install → lifecycle", () => {
  test.skip(isProdApi, "Scenario 1 only runs against DEV/local — Clerk test mode is DEV-only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch(() => undefined);
    createdEmail = null;
  });

  test("fresh signup walks install → observe → bill → halt entirely in the UI", async ({
    page,
  }) => {
    const email = uniqueEmail();
    createdEmail = email;

    // Clerk hosted SignUp form + OTP. Mirrors signup.spec.ts so drift in
    // Clerk's component lives in one place.
    await page.goto("/sign-up");
    await page.getByLabel("Email address", { exact: true }).fill(email);
    await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await page.getByRole("button", { name: /continue|sign up/i }).first().click();

    const otpInput = page.locator('input[autocomplete="one-time-code"]').first();
    await otpInput.waitFor({ timeout: SIGNUP_TIMEOUT_MS });
    await otpInput.fill(TEST_OTP);
    const continueBtn = page.getByRole("button", { name: /continue|verify/i });
    if (await continueBtn.first().isVisible()) await continueBtn.first().click();

    await page.waitForURL(
      (url) => !url.toString().includes("/sign-up") && !url.toString().includes("/sign-in"),
      { timeout: SIGNUP_TIMEOUT_MS },
    );

    // Dashboard /zombies renders auto-provisioned workspace + empty state.
    // First-deploy regression surface lives here (route-guard chain on a
    // brand-new tenant, WorkspaceSwitcher with the auto-provisioned default).
    await page.goto("/zombies");
    await expect(page.getByRole("heading", { name: /zombies/i }).first()).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    await expect(page.getByText(/no zombies yet/i)).toBeVisible();

    // Install via dashboard form. Browser-driven; no API short-cut.
    const name = uniqueName();
    const zombieId = await installViaUI(page, name);

    // Post-install: the form redirects to /zombies/${id}. Recent Activity
    // section is the section-scaffolding assertion (matches logs-detail's
    // downgrade — section presence, not payload contents).
    await expect(page).toHaveURL(new RegExp(`/zombies/${zombieId}(\\?|$)`));
    await expect(page.getByLabel("Recent Activity")).toBeVisible();

    // Listing shows the new row live.
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "live");

    // Billing page renders the balance card (starter credit).
    await page.goto("/settings/billing");
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    // Lifecycle: Stop → Resume → Kill, each via the AlertDialog confirm.
    await page.goto(`/zombies/${zombieId}`);
    await stopZombie(page);
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "parked");

    await page.goto(`/zombies/${zombieId}`);
    await resumeZombie(page);
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "live");

    await page.goto(`/zombies/${zombieId}`);
    await killZombie(page);
    await expectDetailKilled(page);
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "failed");
  });
});
