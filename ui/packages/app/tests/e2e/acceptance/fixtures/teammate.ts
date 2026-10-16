/**
 * teammate.ts — a fresh person who joins the admin fixture's account.
 *
 * Each caller signs up its own invitee, so no shared fixture's workspace list
 * changes under a parallel spec. The address is in Resend's test inbox, so the
 * invite email the owner's action sends is accepted and delivered nowhere, and
 * it matches `PER_RUN_FIXTURE_RE`, so the global sweep reaps one a failed
 * cleanup leaves. DEV only: Clerk's test mode is what lets `+clerk_test` skip
 * the emailed code.
 */
import type { Browser, BrowserContext, Page } from "@playwright/test";
import type { EmailStatus } from "@/lib/api/invites-types";
import type { ClientHandle } from "./api-client";
import * as crypto from "node:crypto";
import * as path from "node:path";
import { clientFor } from "./api-client";
import { deleteUser, findUserIdByEmail } from "./clerk-admin";
import { FIXTURE_KEY, TEST_INBOX_DOMAIN, TEST_INBOX_LOCAL_PREFIX, VERCEL_BYPASS_STATE_FILENAME } from "./constants";
import { signUpAs } from "./signup";

export type OnePage<T> = { items: T[] };
export type InviteRow = { id: string; email: string; link: string; email_status: EmailStatus };
export type Invitee = { email: string; sessionJwt: string; sessionId: string; page: Page; context: BrowserContext };
type MemberRow = { user_id: string; email: string };

export const OWNER_INVITES = "/v1/tenants/me/invites";
export const WAITING_INVITES = "/v1/users/me/invites";
const OWNER_MEMBERS = "/v1/tenants/me/members";
const PASSWORD = "TeamInvitee!2026-stable";
const INVITEE_TAG_BYTES = 4;

export function inviteeEmail(): string {
  return `${TEST_INBOX_LOCAL_PREFIX}${crypto.randomBytes(INVITEE_TAG_BYTES).toString("hex")}@${TEST_INBOX_DOMAIN}`;
}

// Its own browser context: the invitee's session must never share cookies or
// stores with the owner's page.
export async function signUpInvitee(browser: Browser, email: string): Promise<Invitee> {
  const context = await browser.newContext({
    storageState: path.join(process.cwd(), VERCEL_BYPASS_STATE_FILENAME),
  });
  const page = await context.newPage();
  const { sessionJwt, sessionId } = await signUpAs(page, email, PASSWORD);
  return { email, sessionJwt, sessionId, page, context };
}

/** The invitee's API handle; it re-mints its token on the same session when a long walk outlives it. */
export function inviteeHandle(invitee: Invitee): ClientHandle {
  return { sessionJwt: invitee.sessionJwt, sessionId: invitee.sessionId };
}

/** The admin fixture invites `invitee`, who accepts from their own session. */
export async function joinAdminAccount(invitee: Invitee): Promise<void> {
  const invite = await clientFor(FIXTURE_KEY.admin).post<InviteRow>(OWNER_INVITES, { email: invitee.email });
  await clientFor(inviteeHandle(invitee)).post(`${WAITING_INVITES}/${invite.id}/accept`, undefined);
}

/** The admin fixture removes `email` from its account; the person keeps their own sign-in. */
export async function revokeMembership(email: string): Promise<void> {
  const owner = clientFor(FIXTURE_KEY.admin);
  const wanted = email.toLowerCase();
  const members = await owner.get<OnePage<MemberRow>>(OWNER_MEMBERS);
  for (const member of members.items.filter((row) => row.email.toLowerCase() === wanted)) {
    await owner.delete(`${OWNER_MEMBERS}/${member.user_id}`);
  }
}

// Undo whatever a test left in the owner's account, then the invitee.
export async function removeTeammate(email: string, context: BrowserContext | null): Promise<void> {
  const owner = clientFor(FIXTURE_KEY.admin);
  const wanted = email.toLowerCase();
  const loud = (what: string) => (err: unknown) => console.error(`[e2e] team cleanup: ${what} failed:`, err);
  const invites = await owner.get<OnePage<InviteRow>>(OWNER_INVITES).catch(() => ({ items: [] }));
  for (const invite of invites.items.filter((row) => row.email === wanted)) {
    await owner.delete(`${OWNER_INVITES}/${invite.id}`).catch(loud("revoke invite"));
  }
  await revokeMembership(email).catch(loud("remove member"));
  const userId = await findUserIdByEmail(email).catch(() => null);
  if (userId) await deleteUser(userId).catch(loud("delete user"));
  await context?.close();
}
