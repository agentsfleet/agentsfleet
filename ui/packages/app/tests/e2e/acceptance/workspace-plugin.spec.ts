import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { publishFixturePlugin } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref } from "./fixtures/nav";
import {pluginsUiEnabled} from "@/lib/plugin-ui";
import {SETUP_HEADERS} from "@/lib/api/draft-types";

test("an account publication can be removed while Plugin navigation follows its setting", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const name = "Deployment review";
  const entry = await publishFixturePlugin(FIXTURE_KEY.regular, workspace, fixtureDefinition(name));
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(workspaceHref(workspace, "fleets/new"));
  if (!pluginsUiEnabled()) {
    await expect(page.getByRole("navigation", {name: "Primary", exact: true}).getByRole("link", {name: "Plugins", exact: true})).toHaveCount(0);
    expect((await page.goto(workspaceHref(workspace, "plugins")))?.status()).toBe(404);
    await clientFor(FIXTURE_KEY.regular).delete(`/v1/workspaces/${workspace}/plugins/${entry.plugin_id}`, {[SETUP_HEADERS.match]: entry.etag});
  } else {
    await page.getByRole("navigation", {name: "Primary", exact: true}).getByRole("link", {name: "Plugins", exact: true}).click();
    await expect(page).toHaveURL(new RegExp(`/w/${workspace}/plugins$`));
    const row = page.getByRole("listitem").filter({hasText: name});
    await expect(row).toBeVisible();
    await row.getByRole("button", {name: `Manage ${name}`}).click();
    await page.getByRole("dialog").getByRole("button", {name: "Remove from this workspace"}).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText(name);
    await dialog.getByRole("button", {name: "Remove", exact: true}).click();
    await expect(dialog).not.toBeVisible();
    await expect(row).toHaveCount(0);
  }
  const owned = await clientFor(FIXTURE_KEY.regular).get<{items: {id: string}[]}>(
    `/v1/workspaces/${workspace}/plugins?limit=100`);
  expect(owned.items.some((item) => item.id === entry.plugin_id)).toBe(false);
});


test("plugin navigation rejects the removed library route", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await signInAs(page, FIXTURE_KEY.regular);
  const response = await page.goto(workspaceHref(workspace, "library"));
  expect(response?.status()).toBe(404);
  await expect(page.getByRole("heading", {name: "Page not found", exact: true})).toBeVisible();
  const current = await page.goto(workspaceHref(workspace, "plugins"));
  if (!pluginsUiEnabled()) {
    expect(current?.status()).toBe(404);
    await expect(page.getByRole("heading", {name: "Page not found", exact: true})).toBeVisible();
    return;
  }
  await expect(page.getByRole("heading", {name: "Plugins", exact: true})).toBeVisible();
  await expect(page.getByRole("link", {name: "Create in chat", exact: true})).toBeVisible();
});
