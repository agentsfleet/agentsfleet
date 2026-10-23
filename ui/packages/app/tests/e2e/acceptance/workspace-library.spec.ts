import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { publishFixtureLibrary } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { openSettingsDestination, workspaceHref } from "./fixtures/nav";

test("a workspace publication is removed through its reachable Library page", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const name = `library-walk-${crypto.randomUUID().slice(0, 8)}`;
  const entry = await publishFixtureLibrary(FIXTURE_KEY.regular, workspace, fixtureDefinition(name));
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(workspaceHref(workspace, "fleets/new"));
  await openSettingsDestination(page, "Fleet library");
  await page.getByRole("tab", {name: "Workspace", exact: true}).click();
  const row = page.getByRole("listitem").filter({hasText: name});
  await expect(row).toBeVisible();
  await row.getByRole("button", {name: "Remove from this workspace"}).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText(name);
  await dialog.getByRole("button", {name: "Remove", exact: true}).click();
  await expect(dialog).not.toBeVisible();
  await expect(row).toHaveCount(0);
  const owned = await clientFor(FIXTURE_KEY.regular).get<{items: {id: string}[]}>(
    `/v1/workspaces/${workspace}/fleet-libraries/owned?limit=100`);
  expect(owned.items.some((item) => item.id === entry.library_id)).toBe(false);
});
