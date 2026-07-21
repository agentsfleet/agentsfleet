/**
 * fleet-count.spec.ts — `/fleets` live counter tracks row additions.
 *
 * `FleetWall` renders an `aria-label="{N} live"` badge on the page
 * header. This spec seeds fleets one at a time and asserts the counter
 * follows. Catches regressions in `fleetRowState` (status → "live"|"parked"|
 * "failed" mapping), the pulse-cap consolidation, and the
 * `revalidatePath` plumbing that backs the wall.
 *
 * Uses API-side `seedFleet` instead of `installViaUI` — install through
 * the form is already covered end-to-end by login-install-lifecycle and
 * signup-lifecycle. The signal here is the *counter*, not the install
 * action; API seeds keep the spec under 30s.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";

const COUNTER_TIMEOUT_MS = 10_000;

test.describe("live counter increments on install", () => {
  test("each seeded fleet bumps the `{N} live` badge", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "count-");

    await signInAs(page, FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);

    for (let i = 1; i <= 3; i++) {
      const fleet = await seedFleet(FIXTURE_KEY.regular, ws, { name: `count-${tag}-${i}` });
      // The badge counts ACTIVE fleets; a fleet still installing is not live.
      await waitForFleetActive(FIXTURE_KEY.regular, ws, fleet.id);
      await page.goto(workspaceHref(ws, "fleets"));
      await expect(page.getByLabel(`${i} live`)).toBeVisible({ timeout: COUNTER_TIMEOUT_MS });
    }
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "count-");
  });
});
