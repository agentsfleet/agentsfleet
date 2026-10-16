import { expect, test } from "@playwright/test";
import { CreatedMessageSchema } from "@/lib/api/draft-types";
import { CHAT_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { publishFixtureLibrary } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const INSTRUCTIONS = "Acknowledge the request.";
const FIRST_MESSAGE = "Use this recipe as written.";

test("a published recipe is inspected and pins the selected revision in a new private chat", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const name = `recipe-${crypto.randomUUID().slice(0, 8)}`;
  const published = await publishFixtureLibrary(FIXTURE_KEY.regular, workspace, fixtureDefinition(name, INSTRUCTIONS));
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(workspaceHref(workspace, "fleets/new"));
  await expect(page.getByLabel("Private fleet setup conversation")).toBeVisible();
  await page.getByRole("navigation", {name: "Primary"}).getByRole("link", {name: "Fleet library", exact: true}).click();
  await expect(page).toHaveURL(workspaceUrlPattern("library"));
  const inspected = page.waitForResponse((response) => response.url().includes(`revision_id=${published.revision_id}`));
  await page.getByRole("listitem").filter({hasText: name}).getByRole("button", {name: "Inspect", exact: true}).click();
  await expect(page.getByRole("heading", {name: `${name} · revision 1`})).toBeVisible();
  expect((await inspected).ok()).toBe(true);
  await expect(page.getByRole("main")).toContainText(INSTRUCTIONS);
  await page.getByRole("button", {name: "Start chat from this revision"}).click();
  await expect(page).toHaveURL(new RegExp(`/fleets/new\\?.*revision=${published.revision_id}`));
  // Review setup appears only once the chat holds a candidate, so the first message is what binds the pin.
  const created = page.waitForResponse((response) => response.request().method() === "POST"
    && new URL(response.url()).pathname.endsWith("/drafts"));
  await page.getByLabel(CHAT_PLACEHOLDER).fill(FIRST_MESSAGE);
  await page.getByRole("button", {name: "Send message", exact: true}).click();
  const response = await created;
  expect(response.request().postDataJSON()).toMatchObject({
    source: {kind: "library", library_id: published.library_id, revision_id: published.revision_id}, message: FIRST_MESSAGE,
  });
  expect(response.ok()).toBe(true);
  const draft = CreatedMessageSchema.parse(await response.json());
  expect(draft.candidate?.definition.instructions_md).toBe(INSTRUCTIONS);
});
