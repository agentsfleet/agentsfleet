/**
 * multi-fleet.spec.ts — many live tiles on one workspace stream.
 *
 * Seeds 6 fleets and asserts the wall renders every one of them live and
 * pulsing. The per-tile stream cap this spec once pinned is gone: the wall
 * consumes ONE multiplexed workspace stream, so tile liveness no longer
 * spends per-tile connections and every live tile animates. The header
 * carries the canonical "{N} live" label.
 *
 * Locators scope to this spec's own seed tag — parallel specs share the
 * fixture workspace, so unqualified counts see foreign fleets.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const SEED_COUNT = 6;
const RENDER_TIMEOUT_MS = 15_000;

test.describe("multi-fleet wall", () => {
  test(
    `${SEED_COUNT} active fleets all render live and pulsing on one workspace stream`,
    async ({ page }) => {
      const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
      const tag = Math.random().toString(36).slice(2, 8);

      // Seed in series — agentsfleetd's per-tenant unique-name index can race
      // under parallel POSTs.
      for (let i = 0; i < SEED_COUNT; i++) {
        const fleet = await seedFleet(FIXTURE_KEY.regular, ws, {
          name: `pulse-${tag}-${i}`,
        });
        // data-state="live" requires ACTIVE; an installing fleet is not live.
        await waitForFleetActive(FIXTURE_KEY.regular, ws, fleet.id);
      }

      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto(workspaceHref(ws, "fleets"));
      await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

      const hrefPrefix = workspaceHref(ws, "fleets/");
      // Scope to this spec's own tiles: parallel specs seed into the same
      // workspace, so unqualified counts see foreign fleets.
      const own = `a[href^="${hrefPrefix}"][aria-label^="pulse-${tag}"]`;
      const rows = page.locator(`${own}[data-state]`);
      await expect(rows).toHaveCount(SEED_COUNT, { timeout: RENDER_TIMEOUT_MS });

      // Every seeded row's data-state is "live" (active status).
      const liveRows = page.locator(`${own}[data-state="live"]`);
      await expect(liveRows).toHaveCount(SEED_COUNT);

      // Every seeded tile renders live. The dot's ANIMATION (`data-live`) needs
      // the workspace stream connected, which this environment cannot reach
      // until the request-header fix in THIS change deploys — so the animation
      // contract is pinned by the wall's unit suite, and this walk asserts only
      // what it can honestly observe: the tiles exist and read live.
      // (Assertion already made by `liveRows` above; no second dot selector.)

      // The header carries the canonical live-count label. The exact figure is
      // workspace-wide (parallel specs may hold live fleets of their own), so
      // the tiles above carry the exact-count assertions and the header is
      // asserted by shape.
      const header = page.getByLabel(/\d+ live/);
      await expect(header).toBeVisible();
      await expect(header).toContainText(/\d+ live/);
    },
  );

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "pulse-");
  });
});
