/**
 * install-fleet-seed.spec.ts — sanity check for the API seed helper.
 *
 * Sister to install-fleet-cli.spec.ts (canonical install path drives
 * `agentsfleet install`); this spec exercises the API-seed shortcut every
 * later spec (lifecycle, kill, multi-fleet, multi-workspace, events,
 * logs-detail) relies on as setup. If the seed helper drifts, every
 * downstream spec fails the same way — keeping a dedicated sanity test
 * isolates the failure mode.
 *
 * Wire: fixture-user Bearer → POST /v1/workspaces/{ws}/fleets → dashboard
 * /fleets reload → assert the row renders with `data-state="live"`.
 * No `agentsfleet` here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

test.describe("install-fleet-seed", () => {
  test("API-seeded fleet renders on /fleets with live state", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    // Random suffix avoids (workspace_id, name) uniqueness collision with
    // any killed-but-not-deleted row from a previous interrupted run.
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `install-seed-${tag}`;

    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, { name });
    expect(seeded.id).toBeTruthy();

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

    // The row is an anchor `<Link href="/w/{ws}/fleets/{id}" data-state="live">`
    // wrapping a `<div class="font-medium truncate">{name}</div>`. Match by
    // visible name (accessible to a Playwright user) and assert data-state
    // is "live" (the dashboard's translation of agentsfleetd's "active" status —
    // canonical mapping at app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx).
    const row = page.locator(`a[href="${workspaceHref(ws, `fleets/${seeded.id}`)}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "live");
    await expect(row.getByText(name)).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
