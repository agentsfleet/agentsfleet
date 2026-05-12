/**
 * zombie-count.spec.ts — `/zombies` live counter tracks row additions.
 *
 * `ZombiesList` renders an `aria-label="{N} live"` badge on the page
 * header. This spec seeds zombies one at a time and asserts the counter
 * follows. Catches regressions in `liveStateOf` (status → "live"|"parked"|
 * "failed" mapping), the pulse-cap consolidation, and the
 * `revalidatePath` plumbing that backs the dashboard listing.
 *
 * Uses API-side `seedZombie` instead of `installViaUI` — install through
 * the form is already covered end-to-end by login-install-lifecycle and
 * signup-lifecycle. The signal here is the *counter*, not the install
 * action; API seeds keep the spec under 30s.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

const COUNTER_TIMEOUT_MS = 10_000;

test.describe("live counter increments on install", () => {
  test("each seeded zombie bumps the `{N} live` badge", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);

    await signInAs(page, FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);

    for (let i = 1; i <= 3; i++) {
      await seedZombie(FIXTURE_KEY.regular, ws, { name: `count-${tag}-${i}` });
      await page.goto("/zombies");
      await expect(page.getByLabel(`${i} live`)).toBeVisible({ timeout: COUNTER_TIMEOUT_MS });
    }
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
