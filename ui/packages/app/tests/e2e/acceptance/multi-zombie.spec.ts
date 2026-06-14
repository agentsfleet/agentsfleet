/**
 * multi-zombie.spec.ts — pulse-cap contract.
 *
 * Seeds 6 zombies in the fixture user's workspace (more than `PULSE_CAP=5`
 * in `ZombiesList.tsx`). Asserts the listing renders all 6 rows but only
 * the first 5 carry `data-live="true"` on the WakePulse — the 6th gets a
 * static glow. The header consolidates with the canonical "{N} live"
 * label and a single brand WakePulse next to it.
 *
 * Why this matters: the pulse-cap contract is the only way to keep the
 * dashboard from turning into rave-mode for power users with many active
 * zombies. A future change that lifts the cap regresses this assertion.
 *
 * Note on status: seedZombie creates rows that start in `active` status
 * (agentsfleetd's default for new installs). All 6 are therefore "live" as far
 * as `liveStateOf()` is concerned, which is what the pulse-cap actually
 * exercises.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

const SEED_COUNT = 6;
const PULSE_CAP = 5;
const RENDER_TIMEOUT_MS = 15_000;

test.describe("multi-zombie pulse cap", () => {
  test(
    `${SEED_COUNT} active zombies → exactly ${PULSE_CAP} pulse + 1 static glow + header counter`,
    async ({ page }) => {
      const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
      const tag = Math.random().toString(36).slice(2, 8);

      // Seed in series — agentsfleetd's per-tenant unique-name index can race
      // under parallel POSTs.
      for (let i = 0; i < SEED_COUNT; i++) {
        await seedZombie(FIXTURE_KEY.regular, ws, { name: `pulse-${tag}-${i}` });
      }

      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto("/zombies");
      await expect(page).toHaveURL(/\/zombies(\?|$)/);

      const rows = page.locator('a[href^="/zombies/"][data-state]');
      await expect(rows).toHaveCount(SEED_COUNT, { timeout: RENDER_TIMEOUT_MS });

      // Every seeded row's data-state is "live" (active status).
      const liveRows = page.locator('a[href^="/zombies/"][data-state="live"]');
      await expect(liveRows).toHaveCount(SEED_COUNT);

      // Pulse cap: only the first N rows have a child WakePulse with
      // data-live="true". The remainder render the static glow variant.
      const pulsing = page.locator('a[href^="/zombies/"][data-state="live"] [data-live]');
      await expect(pulsing).toHaveCount(PULSE_CAP);

      // Header label consolidates: "{N} live · capped at {PULSE_CAP}" when
      // liveTotal > PULSE_CAP. Match the live count + the cap suffix.
      const header = page.getByLabel(`${SEED_COUNT} live`);
      await expect(header).toBeVisible();
      await expect(header).toContainText(`${SEED_COUNT} live`);
      await expect(header).toContainText(`capped at ${PULSE_CAP}`);
    },
  );

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
