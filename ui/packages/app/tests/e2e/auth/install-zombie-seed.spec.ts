/**
 * install-zombie-seed.spec.ts — sanity check for the API seed helper.
 *
 * Sister to install-zombie-cli.spec.ts (canonical install path drives
 * `zombiectl install`); this spec exercises the API-seed shortcut every
 * later spec (lifecycle, kill, multi-zombie, multi-workspace, events,
 * logs-detail) relies on as setup. If the seed helper drifts, every
 * downstream spec fails the same way — keeping a dedicated sanity test
 * isolates the failure mode.
 *
 * Wire: fixture-user Bearer → POST /v1/workspaces/{ws}/zombies → dashboard
 * /zombies reload → assert the row renders with `data-state="live"`.
 * No `zombiectl` here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("install-zombie-seed", () => {
  test("API-seeded zombie renders on /zombies with live state", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    // Random suffix avoids (workspace_id, name) uniqueness collision with
    // any killed-but-not-deleted row from a previous interrupted run.
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `install-seed-${tag}`;

    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, { name });
    expect(seeded.id).toBeTruthy();

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/zombies");
    await expect(page).toHaveURL(/\/zombies(\?|$)/);

    // The row is an anchor `<Link href="/zombies/{id}" data-state="live">`
    // wrapping a `<div class="font-medium truncate">{name}</div>`. Match by
    // visible name (accessible to a Playwright user) and assert data-state
    // is "live" (the dashboard's translation of zombied's "active" status —
    // canonical mapping at app/(dashboard)/zombies/components/ZombiesList.tsx).
    const row = page.locator(`a[href="/zombies/${seeded.id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "live");
    await expect(row.getByText(name)).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
