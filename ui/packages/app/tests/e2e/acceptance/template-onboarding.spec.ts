import { expect, test } from "@playwright/test";
import { DraftSchema } from "@/lib/api/draft-types";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { publishFixturePlugin } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import {pluginsUiEnabled} from "@/lib/plugin-ui";

const INSTRUCTIONS = "Acknowledge the request.";
const FIRST_MESSAGE = "Use this recipe as written.";

test("a published setup retains its pinned revision while UI reuse follows its setting", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const name = `recipe-${crypto.randomUUID().slice(0, 8)}`;
  const published = await publishFixturePlugin(FIXTURE_KEY.regular, workspace, fixtureDefinition(name, INSTRUCTIONS));
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(workspaceHref(workspace, "fleets/new"));
  await expect(page.getByLabel(CHAT_PLACEHOLDER)).toBeEnabled();
  if (!pluginsUiEnabled()) {
    await expect(page.getByRole("navigation", {name: "Primary", exact: true}).getByRole("link", {name: "Plugins", exact: true})).toHaveCount(0);
    const query = new URLSearchParams({plugin: published.plugin_id, revision: published.revision_id});
    // The route streams its loading frame first, so the refusal is the not-found page, not a 404 status.
    await page.goto(`${workspaceHref(workspace, "fleets/new")}?${query}`);
    await expect(page.getByRole("heading", {name: "Page not found", exact: true})).toBeVisible();
    const entry = await clientFor(FIXTURE_KEY.regular).get<{head: {id: string}}>(`/v1/workspaces/${workspace}/plugins/${published.plugin_id}?revision_id=${published.revision_id}`);
    expect(entry.head.id).toBe(published.revision_id);
    return;
  }
  await page.getByRole("navigation", {name: "Primary", exact: true}).getByRole("link", {name: "Plugins", exact: true}).click();
  await expect(page).toHaveURL(workspaceUrlPattern("plugin"));
  const inspected = page.waitForResponse((response) => response.url().includes(`revision_id=${published.revision_id}`));
  await page.getByRole("button", {name: `Manage ${name}`, exact: true}).click();
  await expect(page.getByRole("dialog").getByRole("heading", {name, exact: true})).toBeVisible();
  expect((await inspected).ok()).toBe(true);
  await page.getByRole("button", {name: "Inspect setup", exact: true}).click();
  await expect(page.getByRole("dialog")).toContainText(INSTRUCTIONS);
  await page.getByRole("button", {name: "Use for new fleet", exact: true}).click();
  await expect(page).toHaveURL(new RegExp(`/fleets/new\\?.*revision=${published.revision_id}`));
  // inline setup appears only once the chat holds a candidate, so the first message is what binds the pin.
  const draftsPath = `/v1/workspaces/${workspace}/drafts`;
  const created = page.waitForResponse((response) => response.request().method() === "POST"
    && new URL(response.url()).pathname.endsWith(draftsPath));
  await page.getByLabel(CHAT_PLACEHOLDER).fill(FIRST_MESSAGE);
  await page.getByRole("button", {name: "Send message", exact: true}).click();
  const response = await created;
  expect(response.request().postDataJSON()).toMatchObject({
    source: {kind: "plugin", plugin_id: published.plugin_id, revision_id: published.revision_id}, message: FIRST_MESSAGE,
  });
  expect(response.status()).toBe(201);
  const location = await response.headerValue("location");
  const draftId = DraftSchema.shape.id.parse(location?.split("/").at(-1));
  const draftPath = `${draftsPath}/${draftId}`;
  expect(location).toBe(draftPath);
  const draft = DraftSchema.parse(await clientFor(FIXTURE_KEY.regular).get<unknown>(draftPath));
  expect(draft.candidate?.definition.instructions_md).toBe(INSTRUCTIONS);
});
