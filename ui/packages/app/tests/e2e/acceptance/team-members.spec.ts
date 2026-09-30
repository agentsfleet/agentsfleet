/**
 * team-members.spec.ts — an owner invites a teammate from Settings → Members;
 * the teammate accepts and finds the owner's workspace under the owner's
 * account in the switcher; the owner removes them again.
 *
 * The owner is the persistent `admin` fixture, a tenant owner. The invitee is a
 * fresh signup per test, so no shared fixture's workspace list changes under a
 * parallel spec. Its address matches `PER_RUN_FIXTURE_RE`, so the global sweep
 * reaps one a failed cleanup leaves. DEV only, like every signup spec: Clerk's
 * test mode is what lets `+clerk_test` skip the emailed code.
 */
import * as crypto from "node:crypto";
import * as path from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { FIXTURE_KEY, VERCEL_BYPASS_STATE_FILENAME } from "./fixtures/constants";
import { signUpAs } from "./fixtures/signup";

const PASSWORD = "TeamInvitee!2026-stable";
const FLOW_TIMEOUT_MS = 120_000;
const OWNER_INVITES = "/v1/tenants/me/invites";
const OWNER_MEMBERS = "/v1/tenants/me/members";
const WAITING_INVITES = "/v1/me/invites";
const OWNER_WORKSPACES = "/v1/tenants/me/workspaces?limit=100";
const OWNER_ROLE = "owner";

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

type OnePage<T> = { items: T[] };
type InviteRow = { id: string; email: string; link: string };
type MemberRow = { user_id: string; email: string };
type WorkspaceRow = { id: string; name: string | null; role: string };
type WaitingRow = { id: string; account: { owner_name: string } };

type Invitee = { email: string; sessionJwt: string; page: Page; context: BrowserContext };

function inviteeEmail(): string {
  return `team-invitee-${crypto.randomBytes(4).toString("hex")}+clerk_test@e2e.agentsfleet.net`;
}

function rowFor(page: Page, email: string) {
  return page.getByRole("row", { name: new RegExp(email.replace(/[.+]/g, "\\$&"), "i") });
}

// Its own browser context: the invitee's session must never share cookies or
// stores with the owner's page.
async function signUpInvitee(browser: Browser, email: string): Promise<Invitee> {
  const context = await browser.newContext({
    storageState: path.join(process.cwd(), VERCEL_BYPASS_STATE_FILENAME),
  });
  const page = await context.newPage();
  const { sessionJwt } = await signUpAs(page, email, PASSWORD);
  return { email, sessionJwt, page, context };
}

// Undo whatever this test left in the owner's account, then the invitee.
async function cleanUp(email: string, context: BrowserContext | null): Promise<void> {
  const owner = clientFor(FIXTURE_KEY.admin);
  const wanted = email.toLowerCase();
  const loud = (what: string) => (err: unknown) => console.error(`[e2e] team cleanup: ${what} failed:`, err);
  const invites = await owner.get<OnePage<InviteRow>>(OWNER_INVITES).catch(() => ({ items: [] }));
  for (const invite of invites.items.filter((row) => row.email === wanted)) {
    await owner.delete(`${OWNER_INVITES}/${invite.id}`).catch(loud("revoke invite"));
  }
  const members = await owner.get<OnePage<MemberRow>>(OWNER_MEMBERS).catch(() => ({ items: [] }));
  for (const member of members.items.filter((row) => row.email.toLowerCase() === wanted)) {
    await owner.delete(`${OWNER_MEMBERS}/${member.user_id}`).catch(loud("remove member"));
  }
  const userId = await findUserIdByEmail(email).catch(() => null);
  if (userId) await deleteUser(userId).catch(loud("delete user"));
  await context?.close();
}

test.describe("teammates join an account", () => {
  test.skip(isProdApi, "signs up a fresh invitee — Clerk test mode is DEV only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  let email: string | null = null;
  let inviteeContext: BrowserContext | null = null;

  test.afterEach(async () => {
    if (!email) return;
    await cleanUp(email, inviteeContext);
    email = null;
    inviteeContext = null;
  });

  test("test_members_page_owner_journey", async ({ page, browser }) => {
    email = inviteeEmail();
    const invitee = await signUpInvitee(browser, email);
    inviteeContext = invitee.context;

    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await signInAs(page, FIXTURE_KEY.admin);
    await page.goto("/settings/members");
    await expect(page.getByRole("heading", { name: /^members$/i })).toBeVisible();

    // Invite from the dialog, then copy the link the invitee will open.
    await page.getByRole("button", { name: /^invite$/i }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel(/^email$/i).fill(invitee.email);
    await dialog.getByRole("button", { name: /^create invite$/i }).click();
    const ready = page.getByTestId("invite-ready");
    const link = await ready.getByLabel(/^invite link$/i).inputValue();
    expect(link).toMatch(/\/invites\/[0-9a-f-]+$/);
    await ready.getByRole("button", { name: /copy invite link/i }).click();
    await expect(ready.getByRole("button", { name: /^copied$/i })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(link);
    await ready.getByRole("button", { name: /^done$/i }).click();
    await expect(rowFor(page, invitee.email)).toHaveCount(1);

    // The invitee accepts from their own session; the owner then sees a member.
    const inviteId = link.slice(link.lastIndexOf("/") + 1);
    await clientFor({ sessionJwt: invitee.sessionJwt }).post(`${WAITING_INVITES}/${inviteId}/accept`, undefined);
    await page.reload();
    const member = rowFor(page, invitee.email);
    await expect(member).toHaveCount(1);

    // Remove them; the row leaves the page.
    await member.getByRole("button", { name: /^remove /i }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^remove$/i }).click();
    await expect(rowFor(page, invitee.email)).toHaveCount(0);
  });

  test("test_invitee_accept_journey", async ({ browser }) => {
    email = inviteeEmail();
    const invitee = await signUpInvitee(browser, email);
    inviteeContext = invitee.context;
    const owner = clientFor(FIXTURE_KEY.admin);

    const invite = await owner.post<InviteRow>(OWNER_INVITES, { email: invitee.email });
    const waiting = await clientFor({ sessionJwt: invitee.sessionJwt }).get<OnePage<WaitingRow>>(WAITING_INVITES);
    const ownerName = waiting.items.find((row) => row.id === invite.id)?.account.owner_name;
    expect(ownerName).toBeTruthy();
    const ownWorkspaces = (await owner.get<OnePage<WorkspaceRow>>(OWNER_WORKSPACES)).items.filter(
      (row) => row.role === OWNER_ROLE,
    );
    const first = ownWorkspaces[0];
    expect(first).toBeDefined();

    // The link lands on the Invites page; accepting opens the owner's first workspace.
    const { page } = invitee;
    await page.goto(new URL(invite.link).pathname);
    await page.getByRole("button", { name: new RegExp(`^accept invite into ${ownerName}'s account$`, "i") }).click();
    await expect(page).toHaveURL(new RegExp(`/w/${first!.id}/fleets(\\?|$)`));

    // The switcher files it under the owner's account, beside the invitee's own.
    await page.getByTestId("workspace-switcher").click();
    const menu = page.getByRole("menu");
    await expect(menu.getByText(`${ownerName}'s account`, { exact: true })).toBeVisible();
    await expect(menu.getByText("Yours", { exact: true })).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: first!.name ?? "Unnamed workspace" })).toBeVisible();
    await expect(page.getByTestId("invite-notice")).toHaveCount(0);
  });
});
