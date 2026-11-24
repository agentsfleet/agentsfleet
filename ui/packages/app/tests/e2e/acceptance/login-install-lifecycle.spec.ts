/**
 * Full-lifecycle Scenario 2 — persistent fixture login → a first message on
 * New fleet → observe → bill → archive.
 *
 * Sister to signup-lifecycle.spec.ts. Same first message + observation +
 * lifecycle leg; the only difference is the auth prefix: cookie-mount via
 * signInAs (using the persistent `regular` fixture) instead of driving Clerk's
 * hosted SignUp form. Runs on DEV AND PROD — the persistent fixture is
 * provisioned in both Clerk DEV and Clerk PROD by globalSetup.
 *
 * The fleet is named by the daemon, so the spec owns it by its identity and
 * purges exactly that row, never a name prefix a sibling might share.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage } from "./fixtures/probe";
import {
  archiveFleet,
  expectDetailArchived,
  expectSidebarFleetState,
} from "./fixtures/lifecycle";
import { uniqueTag } from "./fixtures/observation";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const FLOW_TIMEOUT_MS = 120_000;

test.describe("login → first message → lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);
  let workspaceId = "";
  let fleetId: string | null = null;

  test.afterEach(async () => {
    if (workspaceId && fleetId) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
    fleetId = null;
  });

  test("test_user_starts_a_fleet_from_its_first_message", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    fleetId = await createFleetViaUI(page, workspaceId, probeMessage(uniqueTag()));

    // The page is the fleet's chat, with nothing to set up first.
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    await expect(page.getByLabel("Fleet chat")).toBeVisible({ timeout: 15_000 });

    // The sidebar lists the new fleet and its active status.
    await expectSidebarFleetState(page, fleetId, "active");

    // Billing page loads the report's period totals.
    await page.goto("/settings/billing");
    await expect(page.getByLabel("Period totals")).toBeVisible();

    // Lifecycle: Archive — terminal, and the history stays readable.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await archiveFleet(page);
    await expectDetailArchived(page);
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectSidebarFleetState(page, fleetId, "archived");
  });
});
