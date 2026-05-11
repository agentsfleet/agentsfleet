/**
 * Full-lifecycle Scenario 2 — persistent fixture login → install via dashboard
 * UI → observe → bill → halt.
 *
 * Sister to signup-lifecycle.spec.ts. Same install + observation + lifecycle
 * leg; the only difference is the auth prefix: cookie-mount via signInAs
 * (using the persistent `regular` fixture) instead of driving Clerk's hosted
 * SignUp form. Runs on DEV AND PROD — the persistent fixture is provisioned
 * in both Clerk DEV and Clerk PROD by globalSetup.
 *
 * Why cookie-mount instead of the sign-in form: form-driven login is covered
 * by signup.spec.ts (in DEV). Scenario 2 is about the lifecycle after auth,
 * not the login mechanism — cookie-mount keeps every PROD deploy run fast
 * and isolates the spec from Clerk SignIn component drift.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { installViaUI } from "./fixtures/install-ui";
import {
  expectDetailKilled,
  expectRowState,
  killZombie,
  resumeZombie,
  stopZombie,
} from "./fixtures/lifecycle";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";

const FLOW_TIMEOUT_MS = 120_000;

function uniqueName(): string {
  return `lifecycle-${crypto.randomBytes(4).toString("hex")}`;
}

test.describe("login → install → lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });

  test("persistent fixture installs via UI then walks observe → bill → halt", async ({ page }) => {
    // Cookie-mount via signInAs — no form-drive needed.
    await signInAs(page, FIXTURE_KEY.regular);

    // Install via dashboard form. Random name avoids the
    // (workspace_id, name) uniqueness collision if a previous interrupted
    // run left a killed-but-not-deleted row.
    const name = uniqueName();
    const zombieId = await installViaUI(page, name);

    // Post-install: form redirects to detail page. Recent Activity section
    // is the section-scaffolding assertion (matches logs-detail downgrade).
    await expect(page).toHaveURL(new RegExp(`/zombies/${zombieId}(\\?|$)`));
    await expect(page.getByLabel("Recent Activity")).toBeVisible();

    // Listing shows the new row live.
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "live");

    // Billing page renders the balance card.
    await page.goto("/settings/billing");
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    // Lifecycle: Stop → Resume → Kill.
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
