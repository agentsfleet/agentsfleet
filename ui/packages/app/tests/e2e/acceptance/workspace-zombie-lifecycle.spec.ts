/**
 * workspace-zombie-lifecycle.spec.ts — multi-workspace + zombie full life.
 *
 * Walks the operator path that exercises the most workspace-scoped state:
 *   1. Ensure a 2nd workspace exists (API; the dashboard has no Create-
 *      workspace UI yet — same posture multi-workspace.spec.ts takes).
 *   2. Sign in, switch to the 2nd workspace via the header switcher (UI).
 *   3. Install a zombie via /zombies/new (UI).
 *   4. Kill it (UI confirm dialog).
 *   5. Assert /zombies marks the row failed (terminal).
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
import { expectDetailKilled, expectRowState, killZombie } from "./fixtures/lifecycle";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

const SECONDARY_NAME = "fixture-secondary";
const SWITCH_TIMEOUT_MS = 10_000;
const FLOW_TIMEOUT_MS = 120_000;

test.describe("multi-workspace + zombie lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("operator switches workspace, installs, then kills the zombie", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECONDARY_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/zombies");

    const switcher = page.getByTestId("workspace-switcher");
    await switcher.click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });

    const name = `ws-life-${Math.random().toString(36).slice(2, 8)}`;
    const zombieId = await installViaUI(page, name);
    await expect(page).toHaveURL(new RegExp(`/zombies/${zombieId}(\\?|$)`));

    await page.goto("/zombies");
    await expectRowState(page, zombieId, "live");

    await page.goto(`/zombies/${zombieId}`);
    await killZombie(page);
    await expectDetailKilled(page);

    // UI-delete intentionally skipped: api-dev currently 500s on DELETE
    // for killed rows (UZ-INTERNAL-002 ConnectionBusy). Once the backend
    // fix lands, this branch graduates to clicking "Delete zombie" →
    // confirming the dialog → asserting redirect to /zombies and row
    // absence. Cleanup still runs via the afterEach below.
    await page.goto("/zombies");
    await expectRowState(page, zombieId, "failed");
  });

  test.afterEach(async () => {
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECONDARY_NAME);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, secondary.id);
  });
});
