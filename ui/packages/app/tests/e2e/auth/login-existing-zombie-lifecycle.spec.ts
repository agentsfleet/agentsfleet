/**
 * Full-lifecycle Scenario 2 — persistent fixture login → existing platform-ops
 * zombie → observe → bill → halt.
 *
 * Sister to signup-platformops-lifecycle.spec.ts. Same observation + lifecycle
 * leg, different prefix: cookie-mount via signInAs instead of the Clerk hosted
 * SignUp form, and a beforeEach-seeded zombie owned by the persistent `regular`
 * fixture. Runs on DEV AND PROD — the persistent fixture is provisioned in
 * both Clerk DEV and Clerk PROD by globalSetup.
 *
 * Why cookie-mount instead of the sign-in form: form-driven login is covered
 * by signup.spec.ts (in DEV) and is overkill for every PROD deploy. Scenario 2
 * is about the post-auth lifecycle, not the login mechanism.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  expectDetailKilled,
  expectRowState,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedPlatformOpsZombie, type Zombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";

const FLOW_TIMEOUT_MS = 120_000;

test.describe("login → platform-ops lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  let workspaceId: string;
  let seeded: Zombie;

  test.beforeEach(async () => {
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    seeded = await seedPlatformOpsZombie(FIXTURE_KEY.regular, workspaceId);
  });

  test.afterEach(async () => {
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, workspaceId);
  });

  test("persistent fixture walks observe → bill → halt on a fresh platform-ops zombie", async ({
    page,
  }) => {
    // Cookie-mount via signInAs — no form-drive needed.
    await signInAs(page, FIXTURE_KEY.regular);

    // Pre-seeded row visible with live state.
    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "live");

    // Detail page renders the Recent Activity section scaffolding (same
    // downgrade as logs-detail.spec.ts — section presence only).
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page.getByLabel("Recent Activity")).toBeVisible();

    // Billing page renders the balance card.
    await page.goto("/settings/billing");
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    // Lifecycle leg: Stop → Resume → Kill.
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
