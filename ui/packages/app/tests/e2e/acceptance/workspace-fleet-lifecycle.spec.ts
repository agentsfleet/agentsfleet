/**
 * workspace-fleet-lifecycle.spec.ts — multi-workspace + fleet full life.
 *
 * Walks the operator path that exercises the most workspace-scoped state:
 *   1. Ensure a 2nd workspace exists (API; the dashboard has no Create-
 *      workspace UI yet — same posture multi-workspace.spec.ts takes).
 *   2. Sign in, switch to the 2nd workspace via the header switcher (UI).
 *   3. Install a fleet via /w/<secondary>/fleets/new (UI).
 *   4. Kill it (UI confirm dialog).
 *   5. Assert /w/<secondary>/fleets marks the row failed (terminal).
 *
 * UI-delete is deliberately NOT asserted here pending a known backend bug
 * (UZ-INTERNAL-002 ConnectionBusy on DELETE for killed rows — see PR
 * Discovery + HANDOFF in the implementation branch). When the backend
 * fix lands, the closing assertion graduates from `expectRowState
 * failed` to the full delete + row-absent check.
 *
 * Why this matters: every existing spec runs on the fixture user's
 * default workspace. A regression in `setActiveWorkspace`'s cookie write,
 * the workspace_id lookup chain, or revalidatePath plumbing only surfaces
 * when the active workspace is NOT the default. Same surface multi-
 * workspace.spec.ts covers for the switcher round-trip, extended to the
 * full lifecycle so the workspace_id stays load-bearing through 4 server
 * actions.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { ensureSecondWorkspace, getDefaultWorkspaceId } from "./fixtures/seed";
import { installViaUI } from "./fixtures/install-ui";
import { expectDetailKilled, expectRowState, killFleet } from "./fixtures/lifecycle";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const SECONDARY_NAME = "fixture-secondary";
const SWITCH_TIMEOUT_MS = 10_000;
const FLOW_TIMEOUT_MS = 120_000;

test.describe("multi-workspace + fleet lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("operator switches workspace, installs, then kills the fleet", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECONDARY_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    // Land on the primary (default) workspace first; the switcher then
    // navigates to the secondary workspace's URL segment.
    await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");

    const switcher = page.getByTestId("workspace-switcher");
    await switcher.click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });

    // Onboard + install against the secondary workspace — the one now active
    // in the browser, so the onboarded card renders on /w/<secondary>/fleets/new.
    const name = `ws-life-${Math.random().toString(36).slice(2, 8)}`;
    const fleetId = await installViaUI(page, name, {
      handle: FIXTURE_KEY.regular,
      workspaceId: secondary.id,
    });
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));

    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectRowState(page, fleetId, "live");

    await page.goto(workspaceHref(secondary.id, `fleets/${fleetId}`));
    await killFleet(page);
    await expectDetailKilled(page);

    // UI-delete intentionally skipped: api-dev currently 500s on DELETE
    // for killed rows (UZ-INTERNAL-002 ConnectionBusy). Once the backend
    // fix lands, this branch graduates to clicking "Delete fleet" →
    // confirming the dialog → asserting redirect to /w/<secondary>/fleets and row
    // absence. Cleanup still runs via the afterEach below.
    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectRowState(page, fleetId, "failed");
  });

  test.afterEach(async () => {
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECONDARY_NAME);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, secondary.id);
  });
});
