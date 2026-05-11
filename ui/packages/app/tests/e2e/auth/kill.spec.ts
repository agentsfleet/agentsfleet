/**
 * kill.spec.ts — operator kills a running zombie via the dashboard.
 *
 * Wire: API-seed → /zombies/[id] → KillSwitch "Kill" → ConfirmDialog
 * confirm → return to /zombies and assert the row's `data-state` is
 * `failed` (the dashboard's translation of zombied's `killed` status,
 * per `liveStateOf` in
 * `app/(dashboard)/zombies/components/ZombiesList.tsx:19`).
 *
 * Sister to lifecycle.spec.ts; both exercise the same KillSwitch +
 * ConfirmDialog wiring but with different target statuses. Killing is
 * terminal — the detail page's action panel collapses to a disabled
 * "Killed" indicator afterwards (no Resume / Stop / Kill options). The
 * shared interaction lives in fixtures/lifecycle.ts.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectDetailKilled, expectRowState, killZombie } from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("kill", () => {
  test("Kill transitions the row's data-state from live to failed (terminal)", async ({
    page,
  }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `kill-${tag}`;
    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page).toHaveURL(new RegExp(`/zombies/${seeded.id}(\\?|$)`));

    await killZombie(page);
    await expectDetailKilled(page);

    // Dashboard listing: row still appears (list.zig does not filter killed
    // rows) but state dot is `failed`.
    await page.goto("/zombies");
    await expectRowState(page, seeded.id, "failed");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
