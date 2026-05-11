/**
 * lifecycle.spec.ts — operator stops a running zombie via the dashboard.
 *
 * Wire: API-seed → /zombies/[id] → KillSwitch "Stop" → ConfirmDialog
 * confirm → return to /zombies and assert the row's `data-state` is
 * `parked` (the dashboard's translation of zombied's `stopped` status,
 * per `liveStateOf` in
 * `app/(dashboard)/zombies/components/ZombiesList.tsx:19`).
 *
 * Sister to kill.spec.ts; both exercise the same KillSwitch + ConfirmDialog
 * wiring but with different target statuses (`stopped` vs `killed`). The
 * shared interaction lives in fixtures/lifecycle.ts.
 *
 * Why no `waitForResponse(... PATCH)`: post-WS-A, KillSwitch fires
 * `setZombieStatusAction` (a Next.js Server Action) which POSTs to the app
 * origin, not directly to zombied. The PATCH happens server-side inside the
 * action. Asserting on the dashboard listing's `data-state` is the only
 * stable signal from the browser.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectRowState, stopZombie } from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("lifecycle", () => {
  test("Stop transitions the row's data-state from live to parked", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `lifecycle-${tag}`;
    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page).toHaveURL(new RegExp(`/zombies/${seeded.id}(\\?|$)`));

    await stopZombie(page);

    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "parked");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
