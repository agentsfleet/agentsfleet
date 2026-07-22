/**
 * kill.spec.ts — operator kills a running fleet via the dashboard.
 *
 * Wire: API-seed → /w/[workspaceId]/fleets/[id] → KillSwitch "Kill" → ConfirmDialog
 * confirm → return to /w/[workspaceId]/fleets and assert the row's `data-state` is
 * `failed` (the dashboard's translation of agentsfleetd's `killed` status,
 * per `fleetRowState` in
 * `app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx`).
 *
 * Sister to lifecycle.spec.ts; both exercise the same KillSwitch +
 * ConfirmDialog wiring but with different target statuses. Killing is
 * terminal — the detail page's action panel collapses to a disabled
 * "Killed" indicator afterwards (no Resume / Stop / Kill options). The
 * shared interaction lives in fixtures/lifecycle.ts.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectDetailKilled, expectRowState, killFleet } from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

test.describe("kill", () => {
  test("Kill transitions the row's data-state from live to failed (terminal)", async ({
    page,
  }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `kill-${tag}`;
    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, `fleets/${seeded.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));

    await killFleet(page);
    await expectDetailKilled(page);

    // Dashboard listing: row still appears (list.zig does not filter killed
    // rows) but state dot is `failed`.
    await page.goto(workspaceHref(ws, "fleets"));
    await expectRowState(page, seeded.id, "failed");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "kill-");
  });
});
