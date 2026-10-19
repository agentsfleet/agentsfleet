/**
 * workspace-chat-privacy.spec.ts — a teammate's private setup chat never
 * reaches anyone else's screen, and never outlives their access to it.
 *
 * A fresh teammate joins the admin fixture's account and writes private text
 * into a chat: once in its setup and once as a message the composer sends. The
 * walk then holds every other screen to an absence of that text — the account
 * owner's, a failed history load's, a deleted chat's, a revoked teammate's,
 * and the next account signed in on the same browser. Each absence reads the
 * whole document (`fixtures/private-chat.ts`), not one region. Signs up a fresh
 * teammate, so DEV only, like every signup spec.
 */
import type { Browser, Page, Route } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { signedInContext, signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { fixtureDefinition } from "./fixtures/definition";
import { createFixtureDraft, deleteFixtureDraft } from "./fixtures/native-setup";
import { workspaceHref } from "./fixtures/nav";
import { uniqueTag } from "./fixtures/observation";
import {
  CHAT_COPY, NEW_FLEET, chatUnavailable, draftHref, expectPrivateTextAbsent, historyEntry, openDraft, saveSetupButton,
  sendFromComposer,
} from "./fixtures/private-chat";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import {
  inviteeEmail, inviteeHandle, joinAdminAccount, removeTeammate, revokeMembership, signUpInvitee, type Invitee,
} from "./fixtures/teammate";

type PrivateChat = { workspaceId: string; draftId: string; setupText: string; sentText: string };

const FLOW_TIMEOUT_MS = 240_000;
const NAME_PREFIX = "private-text-";
const SERVICE_UNAVAILABLE = 503;
const GET = "GET";
const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("a private setup chat stays with its creator", () => {
  test.skip(isProdApi, "signs up a fresh teammate — Clerk test mode is DEV only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("test_chat_subject_recovery_and_privacy", async ({ browser }) => {
    const email = inviteeEmail();
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.admin);
    let invitee: Invitee | null = null;
    const drafts: string[] = [];
    try {
      invitee = await signUpInvitee(browser, email);
      await joinAdminAccount(invitee);
      const chat = await writePrivateChat(invitee, workspaceId);
      drafts.push(chat.draftId);
      await aPendingSendSurvivesReload(invitee.page, chat);
      await theOwnerSeesNone(browser, chat);
      await aFailedHistoryLoadShowsNone(invitee.page, chat);
      await aDeletedChatShowsNone(invitee, workspaceId);
      drafts.push(await aRevokedTeammateLosesTheChatUntilRejoining(invitee, workspaceId, email));
      // The switch below signs this teammate's session out, so their chats go while it can still delete them.
      await deleteChats(invitee, workspaceId, drafts.splice(0));
      await theNextAccountSeesNone(invitee.page, chat);
    } finally {
      if (invitee) await deleteChats(invitee, workspaceId, drafts);
      await removeTeammate(email, invitee?.context ?? null);
    }
  });
});

async function writePrivateChat(invitee: Invitee, workspaceId: string): Promise<PrivateChat> {
  const tag = uniqueTag();
  const setupText = `private setup ${tag}`;
  const definition = fixtureDefinition(`${NAME_PREFIX}${tag}`, setupText);
  const draft = await createFixtureDraft(inviteeHandle(invitee), workspaceId, definition);
  return { workspaceId, draftId: draft.id, setupText, sentText: `private message ${tag}` };
}

// A sent message is durable before any reply: a reload brings it back from the server.
async function aPendingSendSurvivesReload(page: Page, chat: PrivateChat): Promise<void> {
  await openDraft(page, chat.workspaceId, chat.draftId);
  await sendFromComposer(page, chat.sentText);
  const conversation = page.getByRole("log");
  await expect(conversation.getByText(chat.sentText)).toBeVisible();
  await page.reload();
  await expect(conversation.getByText(chat.sentText)).toBeVisible();
}

// The account owner shares the workspace, not the chat: no history entry, no text, and its address refuses.
async function theOwnerSeesNone(browser: Browser, chat: PrivateChat): Promise<void> {
  const { context, page } = await signedInContext(browser, FIXTURE_KEY.admin);
  try {
    await page.goto(workspaceHref(chat.workspaceId, NEW_FLEET));
    await expect(page.getByLabel(CHAT_COPY.composer)).toBeVisible();
    await expect(historyEntry(page, chat.draftId)).toHaveCount(0);
    await expectNoPrivateText(page, chat, "owner's chat page");
    await page.goto(draftHref(chat.workspaceId, chat.draftId));
    await expect(chatUnavailable(page)).toBeVisible();
    await expectNoPrivateText(page, chat, "owner opening the teammate's chat address");
  } finally {
    await context.close();
  }
}

// A history load that fails shows that it failed, never a stale or partial list.
async function aFailedHistoryLoadShowsNone(page: Page, chat: PrivateChat): Promise<void> {
  const fail = async (route: Route): Promise<void> => {
    if (route.request().method() === GET) await route.fulfill({ status: SERVICE_UNAVAILABLE, json: {} });
    else await route.fallback();
  };
  const chatList = (url: URL): boolean => url.pathname.endsWith(`/live/v1/workspaces/${chat.workspaceId}/drafts`);
  await page.route(chatList, fail);
  try {
    await page.goto(workspaceHref(chat.workspaceId, NEW_FLEET));
    await expect(page.getByText(CHAT_COPY.historyUnavailable)).toBeVisible();
    await expect(historyEntry(page, chat.draftId)).toHaveCount(0);
    await expectNoPrivateText(page, chat, "failed history load");
  } finally {
    await page.unroute(chatList, fail);
  }
}

// A chat its creator deletes is gone from the open page, its address and the history.
async function aDeletedChatShowsNone(invitee: Invitee, workspaceId: string): Promise<void> {
  const deleted = await writePrivateChat(invitee, workspaceId);
  await openDraft(invitee.page, workspaceId, deleted.draftId);
  await deleteFixtureDraft(inviteeHandle(invitee), workspaceId, deleted.draftId);
  await invitee.page.reload();
  await expect(chatUnavailable(invitee.page)).toBeVisible();
  await expectNoPrivateText(invitee.page, deleted, "deleted chat");
  await invitee.page.goto(workspaceHref(workspaceId, NEW_FLEET));
  await expect(invitee.page.getByLabel(CHAT_COPY.composer)).toBeVisible();
  await expect(historyEntry(invitee.page, deleted.draftId)).toHaveCount(0);
}

// Removed mid-chat, the open page drops the chat on its next request; rejoining brings the creator's chat back.
async function aRevokedTeammateLosesTheChatUntilRejoining(invitee: Invitee, workspaceId: string, email: string): Promise<string> {
  const chat = await writePrivateChat(invitee, workspaceId);
  await openDraft(invitee.page, workspaceId, chat.draftId);
  await revokeMembership(email);
  await saveSetupButton(invitee.page).click();
  await expect(chatUnavailable(invitee.page)).toBeVisible();
  await expect(historyEntry(invitee.page, chat.draftId)).toHaveCount(0);
  await expectNoPrivateText(invitee.page, chat, "revoked teammate");
  await joinAdminAccount(invitee);
  await openDraft(invitee.page, workspaceId, chat.draftId);
  return chat.draftId;
}

// The next person signed in on the same browser starts from nothing the last one wrote.
async function theNextAccountSeesNone(page: Page, chat: PrivateChat): Promise<void> {
  await signInAs(page, FIXTURE_KEY.regular);
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await page.goto(workspaceHref(workspaceId, NEW_FLEET));
  await expect(page.getByLabel(CHAT_COPY.composer)).toBeVisible();
  await expect(historyEntry(page, chat.draftId)).toHaveCount(0);
  await expectNoPrivateText(page, chat, "next account on the same browser");
}

async function expectNoPrivateText(page: Page, chat: Pick<PrivateChat, "setupText" | "sentText">, state: string): Promise<void> {
  await expectPrivateTextAbsent(page, chat.setupText, state);
  await expectPrivateTextAbsent(page, chat.sentText, state);
}

async function deleteChats(invitee: Invitee, workspaceId: string, draftIds: readonly string[]): Promise<void> {
  for (const id of draftIds) await deleteFixtureDraft(inviteeHandle(invitee), workspaceId, id).catch(reportCleanup);
}

function reportCleanup(error: unknown): void {
  console.error("[e2e] private chat cleanup failed:", error);
}
