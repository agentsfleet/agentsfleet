/** Secondary-workspace lifecycle journeys retain their exact workspace identity.
 * Archive leaves read-only history. Direct permanent deletion starts from an
 * active fleet with one confirmation. Each fleet is made by a first message on
 * New fleet and named by the daemon, so cleanup reaches exactly the fleets this
 * suite made, by their identities. */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor, FixtureHttpError } from "./fixtures/api-client";
import { ensureSecondWorkspace, getDefaultWorkspaceId, expectFleetActive } from "./fixtures/seed";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage } from "./fixtures/probe";
import { expectDetailArchived, expectSidebarFleetState, archiveFleet, captureLifecycleReview } from "./fixtures/lifecycle";
import { uniqueTag } from "./fixtures/observation";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { FIXTURE_KEY, SECOND_WORKSPACE_NAME } from "./fixtures/constants";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const SWITCH_TIMEOUT_MS = 10_000;
const FLOW_TIMEOUT_MS = 120_000;
test.describe("multi-workspace + fleet lifecycle", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);
  const made: string[] = [];

  test("operator switches workspace, starts a fleet, then archives it", async ({ page }, info) => {
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

    // The first message goes to the secondary workspace — the one now active
    // in the browser, so its New fleet is /w/<secondary>/fleets/new.
    const fleetId = await createFleetViaUI(page, secondary.id, probeMessage(uniqueTag()));
    made.push(fleetId);
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));

    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectSidebarFleetState(page, fleetId, "active");

    await page.goto(workspaceHref(secondary.id, `fleets/${fleetId}`));
    await archiveFleet(page);
    await expectDetailArchived(page);
    await captureLifecycleReview(page, info, "archived");

    // Archive retains the history row in the same workspace.
    await page.goto(workspaceHref(secondary.id, "fleets"));
    await expectSidebarFleetState(page, fleetId, "archived");
  });

  test("an active fleet can be permanently deleted with one confirmation", async ({page}, info) => {
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    await signInAs(page, FIXTURE_KEY.regular);
    const fleetId = await createFleetViaUI(page, secondary.id, probeMessage(uniqueTag()));
    made.push(fleetId);
    await expectFleetActive(FIXTURE_KEY.regular, secondary.id, fleetId);
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
      .locator("summary").filter({hasText: "Your Fleets"}).click();
    await expect(page).toHaveURL(workspaceHref(secondary.id, "fleets/new"));
    await expect(page.locator(`a[href$="/fleets/${fleetId}"]`)).toHaveCount(0);
  });

  test.afterEach(async () => {
    // Identity-scoped: the secondary workspace is shared with the switcher
    // specs, so cleanup must never reach past the fleets this spec made. A
    // fleet the test itself deleted is already gone.
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    for (const fleetId of made.splice(0)) {
      await purgeFixtureFleet(FIXTURE_KEY.regular, secondary.id, fleetId).catch((error: unknown) => {
        if (!(error instanceof FixtureHttpError && error.status === 404)) throw error;
      });
    }
  });
});
