/**
 * Full-lifecycle Scenario 1 — ephemeral signup → workspace auto-provision →
 * install platform-ops → observe → bill → halt.
 *
 * Why this spec exists: the persistent-fixture suite covers each individual
 * lifecycle slice (install, stop, kill, events, billing) but no spec walks a
 * fresh operator from "I just signed up" through to "I just killed a zombie I
 * observed running." This spec exercises the dashboard route-guard chain that
 * runs ONLY when a brand-new tenant first lands (workspace auto-provision,
 * starter credit, empty-state → populated transition).
 *
 * DEV-only — Clerk PROD almost certainly does not have test mode enabled, so
 * the `+clerk_test@mailinator.com` alias would not short-circuit OTP. Mirrors
 * the same isProdApi guard signup.spec.ts uses.
 *
 * The post-signup install step seeds the canonical samples/platform-ops bundle
 * via the API (not the CLI, not the paste form) — see
 * fixtures/seed.ts:seedPlatformOpsZombie. Reason: install mechanisms are
 * covered by install-zombie-{cli,seed}.spec.ts; this spec asserts the
 * post-install dashboard state.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import {
  deleteUser,
  findUserIdByEmail,
  mintTokens,
} from "./fixtures/clerk-admin";
import {
  expectDetailKilled,
  expectRowState,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedPlatformOpsZombie } from "./fixtures/seed";

const PASSWORD = "SignupFixture!2026-stable";
const TEST_OTP = "424242";
const SIGNUP_TIMEOUT_MS = 30_000;
const FLOW_TIMEOUT_MS = 120_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `signup-lifecycle-${tag}+clerk_test@mailinator.com`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.usezombie.com");

test.describe("signup → platform-ops lifecycle", () => {
  test.skip(isProdApi, "Scenario 1 only runs against DEV/local — Clerk test mode is DEV-only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch(() => undefined);
    createdEmail = null;
  });

  test("fresh signup walks install → observe → bill → halt", async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;

    // 1–3: Clerk hosted SignUp form + OTP. Mirrors signup.spec.ts exactly so
    // any drift in Clerk's form is one shared place.
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

    // 4–5: dashboard /zombies renders auto-provisioned workspace + empty state.
    // The first-deploy regression surface lives here (route-guard chain on a
    // brand-new tenant, WorkspaceSwitcher with the auto-provisioned default).
    await page.goto("/zombies");
    await expect(page.getByRole("heading", { name: /zombies/i }).first()).toBeVisible();
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    await expect(page.getByText(/no zombies yet/i)).toBeVisible();

    // 5a: mint a session JWT for the freshly-signed-up user. Clerk's hosted
    // SignUp flow already triggered the real `user.created` webhook to
    // zombied, so the tenant + default workspace exist server-side — no
    // bootstrap replay needed. The signed-in browser carries Clerk's
    // __session cookie, but our API client wants the api-template JWT.
    const clerkUserId = await findUserIdByEmail(email);
    if (!clerkUserId) throw new Error(`Clerk user not found after signup: ${email}`);
    const { sessionJwt } = await mintTokens(clerkUserId);

    // 5b–6: resolve the auto-provisioned workspace and seed platform-ops via
    // the API path. The dashboard's WorkspaceSwitcher in step 4 shows the
    // same workspace.
    const ws = await getDefaultWorkspaceId({ sessionJwt });
    const seeded = await seedPlatformOpsZombie({ sessionJwt }, ws);
    expect(seeded.id).toBeTruthy();

    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "live");

    // 7: detail page renders the Recent Activity section scaffolding. The
    // EventDetail dialog deferral applies — assert section presence, not
    // payload contents (matches logs-detail.spec.ts downgrade).
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page.getByLabel("Recent Activity")).toBeVisible();

    // 8: billing page renders the balance card. Purchase button is disabled
    // pre-v2.1; we don't assert on its state, only that the card exists.
    await page.goto("/settings/billing");
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    // 9–11: lifecycle leg. Stop → Resume → Kill, asserting row state at
    // each transition. Helpers live in fixtures/lifecycle.ts.
    await page.goto(`/zombies/${seeded.id}`);
    await stopZombie(page);
    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "parked");

    await page.goto(`/zombies/${seeded.id}`);
    await resumeZombie(page);
    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "live");

    await page.goto(`/zombies/${seeded.id}`);
    await killZombie(page);
    await expectDetailKilled(page);
    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "failed");
  });
});
