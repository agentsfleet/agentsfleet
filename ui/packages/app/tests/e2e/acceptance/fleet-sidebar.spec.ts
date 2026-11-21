import {expect, test} from "@playwright/test";
import type {Fleet} from "./fixtures/seed";
import {signInAs} from "./fixtures/auth";
import {FIXTURE_KEY} from "./fixtures/constants";
import {fleetNavigation, openFleetFromSidebar, workspaceHref} from "./fixtures/nav";
import {getDefaultWorkspaceId, seedFleet, waitForFleetActive} from "./fixtures/seed";
import {assertDestructiveTargetIsSafe, purgeFixtureFleet} from "./fixtures/teardown";

const PURPOSES = ["sidebar-release-review", "sidebar-issue-triage", "sidebar-deployment-watch"];

test("workspace entry opens the newest saved conversation and history opens an older fleet", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleets: Fleet[] = [];
  try {
    for (const name of PURPOSES) {
      const fleet = await seedFleet(FIXTURE_KEY.regular, workspace, {name});
      fleets.push(fleet);
      await waitForFleetActive(FIXTURE_KEY.regular, workspace, fleet.id);
    }
    const newest = fleets.at(-1);
    const first = fleets[0];
    if (!newest || !first) throw new Error("Sidebar fixtures were not created");
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, "fleets"));
    await expect(page).toHaveURL(workspaceHref(workspace, `fleets/${newest.id}`));
    await expect(page.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    const navigation = await fleetNavigation(page);
    for (const fleet of fleets) {
      const link = navigation.locator(`a[href="${workspaceHref(workspace, `fleets/${fleet.id}`)}"]`);
      await expect(link).toHaveCount(1);
      await expect(link).toContainText(fleet.name);
      await expect(link).toHaveAttribute("title", `${fleet.name} · active`);
    }
    await openFleetFromSidebar(page, workspace, first.id);
    await expect(page.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await expect(navigation.locator(`a[href="${workspaceHref(workspace, `fleets/${first.id}`)}"]`))
      .toHaveAttribute("data-active", "true");
  } finally {
    await page.goto("about:blank");
    assertDestructiveTargetIsSafe();
    for (const fleet of fleets) await purgeFixtureFleet(FIXTURE_KEY.regular, workspace, fleet.id);
  }
});
