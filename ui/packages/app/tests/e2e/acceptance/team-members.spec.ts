/**
 * team-members.spec.ts — an owner invites a teammate from Settings → Members;
 * the teammate accepts and finds the owner's workspace under the owner's
 * account in the switcher; the owner removes them again.
 *
 * The owner is the persistent `admin` fixture, a tenant owner. The invitee is a
 * fresh signup per test, so no shared fixture's workspace list changes under a
 * parallel spec. Its address is in Resend's test inbox, so the invite email
 * the owner's action sends is accepted and delivered nowhere, and it matches
 * `PER_RUN_FIXTURE_RE`, so the global sweep reaps one a failed cleanup leaves.
 * DEV only, like every signup spec: Clerk's test mode is what lets
 * `+clerk_test` skip the emailed code.
 */
import * as crypto from "node:crypto";
import { expect, test, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { EMAIL_STATUS, type EmailStatus } from "@/lib/api/invites-types";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import {
  OWNER_INVITES, WAITING_INVITES, inviteeEmail, removeTeammate, signUpInvitee, type InviteRow, type OnePage,
} from "./fixtures/teammate";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLOW_TIMEOUT_MS = 120_000;
const OWNER_WORKSPACES = "/v1/tenants/me/workspaces?limit=100";
const CURRENT_USER = "/v1/users/me";
// The shared-thread journey's fleet, swept by prefix after each test.
const THREAD_PREFIX = "team-thread-";
const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";
const PERSON_TURN = '[data-role="user"]';
const ASSISTANT_TURN = '[data-role="assistant"]';
const SENDER_LINE = "fleet-message-sender";
// A turn another screen sent lands over the live stream, well inside this.
const LIVE_MS = 15_000;
// A runner leases the turn and the provider answers inside this.
const REPLY_MS = 150_000;

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

// The words the Members table shows for each email status, held here rather
// than imported so the spec pins what a person reads on the page.
const EMAIL_STATUS_LABEL: Record<EmailStatus, string> = {
  [EMAIL_STATUS.sent]: "Invite emailed",
  [EMAIL_STATUS.failed]: "Email not sent",
  [EMAIL_STATUS.unconfigured]: "Email not set up",
};

type WorkspaceRow = { id: string; name: string | null; role: string };
type WaitingRow = { id: string; account: { owner_name: string } };
// `display_name` is absent, not null, for an account that never set one.
type CurrentUser = { display_name?: string };

// A named teammate's turn shows their name; an unnamed one shows no line.
async function expectSenderLine(turn: Locator, name: string | null): Promise<void> {
  const line = turn.getByTestId(SENDER_LINE);
  if (name === null) await expect(line).toHaveCount(0);
  else await expect(line).toHaveText(name);
}

function rowFor(page: Page, email: string) {
  return page.getByRole("row", { name: new RegExp(email.replace(/[.+]/g, "\\$&"), "i") });
}

test.describe("teammates join an account", () => {
  test.skip(isProdApi, "signs up a fresh invitee — Clerk test mode is DEV only");
  test.setTimeout(FLOW_TIMEOUT_MS);

  let email: string | null = null;
  let inviteeContext: BrowserContext | null = null;

  test.afterEach(async () => {
    if (!email) return;
    await removeTeammate(email, inviteeContext);
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
    await dialog.getByRole("button", { name: /^send$/i }).click();
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

  // Which status the row shows depends on the deployment's relay, so the test
  // asks the backend which one it recorded, then holds the row to that label,
  // with "Send again" whenever a relay exists: with none, sending again could
  // only be refused.
  test("test_members_page_shows_email_status", async ({ page }) => {
    email = inviteeEmail();
    const owner = clientFor(FIXTURE_KEY.admin);
    const created = await owner.post<InviteRow>(OWNER_INVITES, { email });
    const recorded = (await owner.get<OnePage<InviteRow>>(OWNER_INVITES)).items.find((row) => row.id === created.id);
    if (recorded === undefined) throw new Error(`invite ${created.id} is missing from the owner's invite list`);
    const status = recorded.email_status;
    await signInAs(page, FIXTURE_KEY.admin);
    await page.goto("/settings/members");
    const row = rowFor(page, email);
    await expect(row).toHaveCount(1);
    // The row carries a second, phone-width copy of the status that CSS hides here.
    await expect(row.getByText(EMAIL_STATUS_LABEL[status], { exact: true }).filter({ visible: true })).toBeVisible();
    const sendAgain = row.getByRole("button", { name: /^send the invite email to .* again$/i });
    await expect(sendAgain).toHaveCount(status === EMAIL_STATUS.unconfigured ? 0 : 1);
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
      (row) => row.role === ACCOUNT_ROLE.owner,
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

  test("test_member_sees_teammate_message_then_reply", async ({ page, browser }) => {
    test.setTimeout(REPLY_MS + FLOW_TIMEOUT_MS);
    email = inviteeEmail();
    const invitee = await signUpInvitee(browser, email);
    inviteeContext = invitee.context;
    const owner = clientFor(FIXTURE_KEY.admin);
    const bobApi = clientFor({ sessionJwt: invitee.sessionJwt });

    // Bob joins John's account, and John's workspace holds a fleet for both.
    const invite = await owner.post<InviteRow>(OWNER_INVITES, { email: invitee.email });
    await bobApi.post(`${WAITING_INVITES}/${invite.id}/accept`, undefined);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.admin);
    const tag = crypto.randomBytes(3).toString("hex");
    const fleet = await seedFleet(FIXTURE_KEY.admin, workspaceId, { name: `${THREAD_PREFIX}${tag}` });
    try {
      await waitForFleetActive(FIXTURE_KEY.admin, workspaceId, fleet.id);
      // A teammate's turn carries their name only when their account has one
      // (lib/events/sender-names.ts), and these fixtures sign up without one,
      // so each sender line is held to what the backend recorded.
      const johnName = (await owner.get<CurrentUser>(CURRENT_USER)).display_name ?? null;
      const bobName = (await bobApi.get<CurrentUser>(CURRENT_USER)).display_name ?? null;

      await signInAs(page, FIXTURE_KEY.admin);
      const thread = workspaceHref(workspaceId, `fleets/${fleet.id}`);
      await page.goto(thread);
      await invitee.page.goto(thread);
      await expect(page.getByLabel(CHAT_LABEL)).toBeVisible();
      await expect(invitee.page.getByLabel(CHAT_LABEL)).toBeVisible();

      // Bob types; John's screen shows it at once, under Bob's name.
      const bobSays = `check the tests ${tag}`;
      await send(invitee.page, bobSays);
      const onJohns = page.getByLabel(CHAT_LABEL).locator(PERSON_TURN).filter({ hasText: bobSays });
      await expect(onJohns).toBeVisible({ timeout: LIVE_MS });
      await expectSenderLine(onJohns, bobName);

      // John answers; Bob's screen names John, and John's own turn names no one.
      const johnSays = `ship it ${tag}`;
      await send(page, johnSays);
      const onBobs = invitee.page.getByLabel(CHAT_LABEL).locator(PERSON_TURN).filter({ hasText: johnSays });
      await expect(onBobs).toBeVisible({ timeout: LIVE_MS });
      await expectSenderLine(onBobs, johnName);
      const ownTurn = page.getByLabel(CHAT_LABEL).locator(PERSON_TURN).filter({ hasText: johnSays });
      await expect(ownTurn.getByTestId(SENDER_LINE)).toHaveCount(0);

      // The fleet's answer to Bob reaches John's screen too.
      await expect(page.getByLabel(CHAT_LABEL).locator(ASSISTANT_TURN).first()).toBeVisible({ timeout: REPLY_MS });
    } finally {
      await cleanWorkspaceFleets(FIXTURE_KEY.admin, workspaceId, THREAD_PREFIX);
    }
  });
});

// Types `text` into the fleet's composer and sends it.
async function send(page: Page, text: string): Promise<void> {
  const composer = page.getByLabel(COMPOSER_LABEL);
  await composer.getByPlaceholder(/message this fleet/i).fill(text);
  await composer.getByRole("button", { name: /send/i }).click();
}
