/** Secondary-workspace lifecycle journeys retain their exact workspace identity.
 * Archive leaves read-only history. Direct permanent deletion starts from an
 * active fleet with one confirmation. Cleanup reaches only this suite's fleets. */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { ensureSecondWorkspace, getDefaultWorkspaceId, waitForFleetActive } from "./fixtures/seed";
import { saveFleetViaUI } from "./fixtures/install-ui";
import { expectDetailArchived, expectRowState, archiveFleet, captureLifecycleReview } from "./fixtures/lifecycle";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY, SECOND_WORKSPACE_NAME } from "./fixtures/constants";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const SWITCH_TIMEOUT_MS = 10_000;
const FLOW_TIMEOUT_MS = 120_000;
// Purpose-based fleet names keep cleanup scoped to this lifecycle suite.
const SEED_PREFIX = "Workspace lifecycle";

test.describe("multi-workspace + fleet lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("operator switches workspace, installs, then archives the fleet", async ({ page }, info) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    // Land on the primary (default) workspace first; the switcher then
    // navigates to the secondary workspace's URL segment.
    await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");

    const switcher = page.getByTestId("workspace-switcher");
    await switcher.click();
    await page.getByRole("menuitem", {name: /^Switch workspace:/}).click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();
    await expect(switcher).toHaveAttribute("aria-label", new RegExp(`${secondary.name ?? "Unnamed workspace"}$`), {
      timeout: SWITCH_TIMEOUT_MS,
    });

    // Onboard + install against the secondary workspace — the one now active
    // in the browser, so the onboarded card renders on /w/<secondary>/fleets/new.
    const fleetId = await saveFleetViaUI(page, SEED_PREFIX, {
      handle: FIXTURE_KEY.regular,
      workspaceId: secondary.id,
    });
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));

    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectRowState(page, fleetId, "live");

    await page.goto(workspaceHref(secondary.id, `fleets/${fleetId}`));
    await archiveFleet(page);
    await expectDetailArchived(page);
    await captureLifecycleReview(page, info, "archived");

    // Archive retains the history row in the same workspace.
    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectRowState(page, fleetId, "archived");
  });

  test("an active fleet can be permanently deleted with one confirmation", async ({page}, info) => {
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    await signInAs(page, FIXTURE_KEY.regular);
    const fleetId = await saveFleetViaUI(page, `${SEED_PREFIX} deletion`, {
      handle: FIXTURE_KEY.regular, workspaceId: secondary.id,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, secondary.id, fleetId);
    await page.reload();
    await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
    await page.getByRole("menuitem", {name: "Permanently delete", exact: true}).click();
    const confirmation = page.getByRole("alertdialog", {name: "Permanently delete fleet?", exact: true});
    await expect(confirmation).toBeVisible();
    await expect(page.getByRole("alertdialog", {name: "Archive fleet?", exact: true})).toHaveCount(0);
    await confirmation.getByRole("button", {name: "Permanently delete", exact: true}).click();
    await expect(page).toHaveURL(workspaceHref(secondary.id, "fleets/new"));
    await expect(page.getByTestId("fleet-notice")).toContainText("Fleet permanently deleted");
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
    await expect(clientFor(FIXTURE_KEY.regular).get(`/v1/workspaces/${secondary.id}/fleets/${fleetId}`))
      .rejects.toMatchObject({status: 404});
    await captureLifecycleReview(page, info, "deleted");
    await page.getByRole("navigation", {name: "Primary", exact: true})
      .getByRole("link", {name: "Your fleets", exact: true}).click();
    await expect(page).toHaveURL(workspaceHref(secondary.id, "fleets"));
    await expect(page.locator(`a[href$="/fleets/${fleetId}"]`)).toHaveCount(0);
  });

  test.afterEach(async () => {
    // Prefix-scoped: the secondary workspace is shared with the switcher
    // specs, so cleanup must never reach past this spec's own seeds.
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, secondary.id, SEED_PREFIX);
  });
});
