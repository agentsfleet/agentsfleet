/**
 * setup-revision-journey.spec.ts — "change the fleet I just saved."
 *
 * A user describes a fleet and saves it, reopens the same private setup chat
 * from the fleet, asks for one change, reviews it and saves again. The fleet
 * moves to its second setup revision, and its chat keeps both saves.
 *
 * A second session opened Edit setup while the fleet was on revision 1. When
 * that session saves instructions after revision 2 has landed, the daemon
 * refuses the save, and the fleet keeps revision 2. Both proposals come from
 * the live authoring model, so each request is complete and asks no questions.
 */
import { expect, test } from "@playwright/test";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { DraftSchema } from "@/lib/api/draft-types";
import { saveDoneText } from "@/lib/events/fleet-activity";
import { clientFor } from "./fixtures/api-client";
import { signedInContext, signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { readFixtureFleet } from "./fixtures/native-setup";
import { workspaceHref } from "./fixtures/nav";
import {
  CHAT_COPY, NEW_FLEET, editSetupAction, openSavedFleet, privateSaveButton, sendFromComposer, setupAction, setupSummary,
} from "./fixtures/private-chat";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const FLEET_NAME = "inbox-acknowledger";
const FIRST_WORD = "ACK";
const SECOND_WORD = "DONE";
const DESCRIPTION_MARK = "replies with one line";
const STALE_INSTRUCTIONS = "Reply with the single word STALE.";
// `afd_setup` refuses a save whose source revision is no longer the fleet's head.
const SOURCE_REVISION_CHANGED = "UZ-AUTHOR-002";
const CONFLICT = 409;
const JOURNEY_TIMEOUT_MS = 600_000;
const PROPOSAL_TIMEOUT_MS = 180_000;

function description(): string {
  return `Create a fleet named ${FLEET_NAME}. When it receives a message it ${DESCRIPTION_MARK}: ` +
    `the word ${FIRST_WORD}, a space, then the message text. It uses no tools, no triggers and no credentials. ` +
    "Do not ask me any questions; propose the complete setup now.";
}

function changeRequest(): string {
  return `Change the reply word from ${FIRST_WORD} to ${SECOND_WORD}. Keep the name, the one-line format ` +
    "and everything else unchanged. Do not ask me any questions; propose the updated setup now.";
}

test.describe("setup revisions", () => {
  let workspaceId = "";
  let fleetId: string | null = null;

  test.afterAll(async () => {
    if (workspaceId && fleetId) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
  });

  test("a saved fleet takes a second setup from the same chat, keeps both saves, and refuses a save from the superseded setup", async ({ page, browser }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await signInAs(page, FIXTURE_KEY.regular);

    // Revision 1: describe, review, save.
    await page.goto(workspaceHref(workspaceId, NEW_FLEET));
    await page.getByLabel(CHAT_PLACEHOLDER).fill(description());
    await page.getByRole("button", { name: CHAT_COPY.start, exact: true }).click();
    // The proposed name heads the page (DraftChatHeading); the setup region holds its checks.
    await expect(page.getByRole("heading", { level: 1, name: FLEET_NAME, exact: true })).toBeVisible({ timeout: PROPOSAL_TIMEOUT_MS });
    await (await privateSaveButton(page)).click();
    const saved = await openSavedFleet(page, { handle: FIXTURE_KEY.regular, workspaceId });
    fleetId = saved;
    const first = await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, saved);
    expect(first.definition?.instructions_md).toContain(FIRST_WORD);
    expect(first.definition?.instructions_md).not.toContain(SECOND_WORD);
    const chat = page.getByLabel("Fleet chat");
    await expect(chat.getByText(saveDoneText("1")).first()).toBeVisible();

    // A second session opens Edit setup while revision 1 is still current.
    const other = await signedInContext(browser, FIXTURE_KEY.regular);
    try {
      const stale = other.page;
      await stale.goto(workspaceHref(workspaceId, `fleets/${saved}`));
      await (await editSetupAction(stale)).click();
      await expect(stale).toHaveURL(new RegExp(`/fleets/new\\?fleet=${saved}&setup_revision=1$`));
      await expect(stale.getByLabel(CHAT_COPY.composer)).toBeVisible();

      // Revision 2: the same chat, with the first request still in it, takes one change.
      await (await editSetupAction(page)).click();
      await expect(page).toHaveURL(new RegExp(`/fleets/new\\?fleet=${saved}&setup_revision=1$`));
      await expect(page.getByText(DESCRIPTION_MARK).first()).toBeVisible();
      const created = page.waitForResponse(response => response.request().method() === "POST"
        && new URL(response.url()).pathname.endsWith(`/workspaces/${workspaceId}/drafts`));
      await sendFromComposer(page, changeRequest());
      const response = await created;
      expect(response.status()).toBe(201);
      const draftId = DraftSchema.shape.id.parse((await response.headerValue("location"))?.split("/").at(-1));
      await expect.poll(async () => DraftSchema.parse(await clientFor(FIXTURE_KEY.regular)
        .get<unknown>(`/v1/workspaces/${workspaceId}/drafts/${draftId}`)).candidate?.definition.instructions_md ?? "",
      { timeout: PROPOSAL_TIMEOUT_MS }).toContain(SECOND_WORD);
      await expect(setupSummary(page)).toContainText("Save changes when");
      await (await privateSaveButton(page)).click();
      expect(await openSavedFleet(page, { handle: FIXTURE_KEY.regular, workspaceId, revision: "2" })).toBe(saved);

      // The fleet chat shows the second save and still holds the first.
      await expect(chat.getByText(saveDoneText("2")).first()).toBeVisible();
      await expect(chat.getByText(saveDoneText("1")).first()).toBeVisible();
      const second = await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, saved);
      expect(second.definition?.instructions_md).toContain(SECOND_WORD);

      // The session still on revision 1 cannot save over revision 2.
      await (await setupAction(stale, CHAT_COPY.editSetup)).click();
      const editor = stale.getByRole("dialog", { name: CHAT_COPY.instructions, exact: true });
      await editor.getByLabel(CHAT_COPY.instructions, { exact: true }).fill(STALE_INSTRUCTIONS);
      const applied = stale.waitForResponse(answer => answer.request().method() === "POST"
        && new URL(answer.url()).pathname.endsWith(`/fleets/${saved}/revisions`));
      await editor.getByRole("button", { name: CHAT_COPY.updateInstructions }).click();
      const refusal = await applied;
      expect(refusal.status()).toBe(CONFLICT);
      const problem: unknown = await refusal.json();
      expect(problem).toMatchObject({ error_code: SOURCE_REVISION_CHANGED });
      await expect(editor.getByRole("alert")).toBeVisible();
      await expect(editor.getByRole("alert")).not.toHaveText(/^\s*$/);
      const after = await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, saved);
      expect(after.setup_revision).toBe("2");
      expect(after.definition?.instructions_md).toBe(second.definition?.instructions_md);
    } finally {
      await other.context.close();
    }
  });
});
