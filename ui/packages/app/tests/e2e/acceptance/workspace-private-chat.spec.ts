/**
 * workspace-private-chat.spec.ts — one person's private setup chat goes from a
 * draft to a saved fleet and back again to improve it; a teammate in another
 * person's account then finds their own chat again.
 *
 * The candidate comes from a definition-source draft created as the signed-in
 * person, because an authoring-model reply is not deterministic
 * (`fixtures/install-ui.ts` makes the same trade). Everything after that runs
 * through the shipped chat: the history entry, a reload, Save from the
 * keyboard, the saved fleet's full authoring page, Save changes, and a
 * phone-width reread. Signs up a fresh teammate, so DEV only, like every
 * signup spec.
 */
import type { Browser, Page } from "@playwright/test";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY, VERCEL_BYPASS_STATE_FILENAME } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { createFixtureDraft, deleteFixtureDraft, readFixtureFleet } from "./fixtures/native-setup";
import { DraftSchema } from "@/lib/api/draft-types";
import { workspaceHref } from "./fixtures/nav";
import { uniqueTag } from "./fixtures/observation";
import {
  CHAT_COPY, NEW_FLEET, draftHref, editSetupLink, historyEntry, openDraft, openSavedFleet, saveSetupButton, setupAction, setupSummary,
} from "./fixtures/private-chat";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { inviteeEmail, inviteeHandle, joinAdminAccount, removeTeammate, signUpInvitee, type Invitee } from "./fixtures/teammate";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLOW_TIMEOUT_MS = 240_000;
// Save and Save changes each run bounded in-request steps before they answer.
const SETUP_TIMEOUT_MS = 60_000;
const NAME_PREFIX = "private-chat-";
const IMPROVED_INSTRUCTIONS = "Acknowledge the request, then name the next step.";
const PHONE = { width: 390, height: 844 } as const;
const FLEET_PAGE = /\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/;
const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("a private fleet setup chat", () => {
  test.skip(isProdApi, "signs up a fresh teammate — Clerk test mode is DEV only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("test_private_draft_save_runtime_and_improve", async ({ page, browser }) => {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const name = `${NAME_PREFIX}${uniqueTag()}`;
    const drafts: string[] = [];
    try {
      const draft = await createFixtureDraft(FIXTURE_KEY.regular, workspaceId, fixtureDefinition(name));
      drafts.push(draft.id);
      await signInAs(page, FIXTURE_KEY.regular);
      await recoverFromHistoryAndReload(page, workspaceId, draft.id);
      await saveFromTheKeyboard(page);
      const fleetId = await openTheSavedFleet(page, workspaceId, name);
      drafts.push(await improveTheSavedFleet(page, workspaceId, fleetId));
      await rereadAtPhoneWidth(browser, workspaceId, draft.id, fleetId);
      await teammateRecoversTheirOwnChat(browser);
    } finally {
      await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, NAME_PREFIX);
      for (const id of drafts) await deleteFixtureDraft(FIXTURE_KEY.regular, workspaceId, id).catch(reportCleanup);
    }
  });
});

// The chat is listed under its owner's history, reopens from that entry, and survives a reload.
async function recoverFromHistoryAndReload(page: Page, workspaceId: string, draftId: string): Promise<void> {
  await openDraft(page, workspaceId, draftId);
  await page.goto(workspaceHref(workspaceId, NEW_FLEET));
  await historyEntry(page, draftId).click();
  await expect(page).toHaveURL(new RegExp(`\\?draft=${draftId}$`));
  await expect(setupSummary(page)).toBeVisible();
  await page.reload();
  await expect(setupSummary(page)).toBeVisible();
}

// Save is reachable from the keyboard in the chat header.
async function saveFromTheKeyboard(page: Page): Promise<void> {
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const save = await saveSetupButton(page);
  await expect(save).toBeEnabled();
  await save.focus();
  await save.press("Enter");
  await openSavedFleet(page);
}

// Explicitly opening the saved fleet and reloading preserves its durable identity.
async function openTheSavedFleet(page: Page, workspaceId: string, name: string): Promise<string> {
  await expect(page).toHaveURL(FLEET_PAGE, { timeout: SETUP_TIMEOUT_MS });
  const fleetId = new URL(page.url()).pathname.split("/").pop();
  if (!fleetId) throw new Error("Open fleet led to no fleet");
  expect((await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId)).name).toBe(name);
  await page.reload();
  await expect(editSetupLink(page)).toBeVisible();
  return fleetId;
}

// Edit setup starts from the fleet header; saving moves the fleet to its next setup revision.
async function improveTheSavedFleet(page: Page, workspaceId: string, fleetId: string): Promise<string> {
  const before = await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
  await editSetupLink(page).click();
  await expect(page).toHaveURL(new RegExp(`/fleets/new\\?fleet=${fleetId}&setup_revision=${before.setup_revision}$`));
  await expect(page.getByLabel(CHAT_COPY.composer)).toBeVisible();
  await (await setupAction(page, CHAT_COPY.editSetup)).click();
  const panel = page.getByRole("dialog", {name: CHAT_COPY.instructions, exact: true});
  await panel.getByLabel(CHAT_COPY.instructions, { exact: true }).fill(IMPROVED_INSTRUCTIONS);
  const created = page.waitForResponse(response => response.request().method() === "POST"
    && new URL(response.url()).pathname.endsWith(`/workspaces/${workspaceId}/drafts`));
  await panel.getByRole("button", { name: CHAT_COPY.updateInstructions }).click();
  const response = await created;
  expect(response.status()).toBe(201);
  const location = await response.headerValue("location");
  const draftId = DraftSchema.shape.id.parse(location?.split("/").at(-1));
  expect(location).toBe(`/v1/workspaces/${workspaceId}/drafts/${draftId}`);
  await expect(panel).toBeHidden({timeout: SETUP_TIMEOUT_MS});
  await openSavedFleet(page, String(BigInt(before.setup_revision) + 1n));
  const after = await readFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
  expect(after.setup_revision).toBe(String(BigInt(before.setup_revision) + 1n));
  expect(after.definition?.instructions_md).toBe(IMPROVED_INSTRUCTIONS);
  return draftId;
}

// A saved chat's address retains authoring at phone width and can open its fleet.
async function rereadAtPhoneWidth(browser: Browser, workspaceId: string, draftId: string, fleetId: string): Promise<void> {
  const context = await browser.newContext({
    viewport: PHONE, isMobile: true, hasTouch: true,
    storageState: path.join(process.cwd(), VERCEL_BYPASS_STATE_FILENAME),
  });
  try {
    const page = await context.newPage();
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(draftHref(workspaceId, draftId));
    await expect(page.getByLabel(CHAT_COPY.composer)).toBeVisible();
    await expectNoPageOverflow(page);
    await expectAccessible(page, "saved setup chat at phone width");
    await page.getByRole("link", {name: CHAT_COPY.openFleet, exact: true}).click();
    await expect(page).toHaveURL(new RegExp(`/fleets/${fleetId}$`));
    await expect(editSetupLink(page)).toBeVisible();
    await expectNoPageOverflow(page);
    await expectAccessible(page, "saved private chat at phone width");
  } finally {
    await context.close();
  }
}

// A teammate who joined the admin's account reopens their own chat from its history entry and after a reload.
async function teammateRecoversTheirOwnChat(browser: Browser): Promise<void> {
  const email = inviteeEmail();
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.admin);
  let invitee: Invitee | null = null;
  let draftId: string | null = null;
  try {
    invitee = await signUpInvitee(browser, email);
    await joinAdminAccount(invitee);
    const draft = await createFixtureDraft(inviteeHandle(invitee), workspaceId, fixtureDefinition(`${NAME_PREFIX}${uniqueTag()}`));
    draftId = draft.id;
    await recoverFromHistoryAndReload(invitee.page, workspaceId, draft.id);
  } finally {
    if (invitee && draftId) await deleteFixtureDraft(inviteeHandle(invitee), workspaceId, draftId).catch(reportCleanup);
    await removeTeammate(email, invitee?.context ?? null);
  }
}

function reportCleanup(error: unknown): void {
  console.error("[e2e] private chat cleanup failed:", error);
}
