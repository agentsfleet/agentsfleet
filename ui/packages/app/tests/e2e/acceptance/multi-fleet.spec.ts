import {expect, test} from "@playwright/test";
import type {Fleet} from "./fixtures/seed";
import {signInAs} from "./fixtures/auth";
import {FIXTURE_KEY} from "./fixtures/constants";
import {fleetNavigation, openFleetFromSidebar, workspaceHref} from "./fixtures/nav";
import {getDefaultWorkspaceId, seedFleet, waitForFleetActive} from "./fixtures/seed";
import {assertDestructiveTargetIsSafe, purgeFixtureFleet} from "./fixtures/teardown";

const PURPOSES = ["release-review", "deployment-watch", "issue-triage", "incident-summary", "change-review", "maintenance-watch"];
const VIEWPORTS = [{width: 1440, height: 900}, {width: 390, height: 844}];

test("saved fleets remain selectable on desktop and mobile without opening background conversations", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleets: Fleet[] = [];
  const streams = new Set<string>();
  page.on("request", request => {
    const path = new URL(request.url()).pathname;
    if (path.endsWith("/events/stream")) streams.add(path);
  });
  try {
    for (const purpose of PURPOSES) {
      const fleet = await seedFleet(FIXTURE_KEY.regular, workspace, {name: `conversation-${purpose}`});
      fleets.push(fleet);
      await waitForFleetActive(FIXTURE_KEY.regular, workspace, fleet.id);
    }
    const newest = fleets.at(-1);
    if (!newest) throw new Error("Conversation fixtures were not created");
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, "fleets"));
    await expect(page).toHaveURL(workspaceHref(workspace, `fleets/${newest.id}`));
    await expect(page.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await expect.poll(() => [...streams]).toEqual([
      `/live/v1/workspaces/${workspace}/fleets/${newest.id}/events/stream`,
    ]);
    for (const viewport of VIEWPORTS) {
      await page.setViewportSize(viewport);
      const navigation = await fleetNavigation(page);
      for (const fleet of fleets) {
        const link = navigation.locator(`a[href="${workspaceHref(workspace, `fleets/${fleet.id}`)}"]`);
        await expect(link).toHaveCount(1);
        await expect(link).toContainText(fleet.name);
        await expect(link).toHaveAttribute("title", `${fleet.name} · active`);
      }
      const target = viewport.width === 390 ? newest : fleets[0];
      if (!target) throw new Error("Conversation target was not created");
      await openFleetFromSidebar(page, workspace, target.id);
      await expect(page.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    }
    const first = fleets[0];
    if (!first) throw new Error("First conversation fixture was not created");
    await expect.poll(() => [...streams].sort()).toEqual([
      `/live/v1/workspaces/${workspace}/fleets/${newest.id}/events/stream`,
      `/live/v1/workspaces/${workspace}/fleets/${first.id}/events/stream`,
    ].sort());
  } finally {
    await page.goto("about:blank");
    assertDestructiveTargetIsSafe();
    for (const fleet of fleets) await purgeFixtureFleet(FIXTURE_KEY.regular, workspace, fleet.id);
  }
});
