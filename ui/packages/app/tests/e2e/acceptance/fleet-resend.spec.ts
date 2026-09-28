/** A send whose answer never reached the page, the Claude.ai way: the row
 * leaves the thread, the text returns to the composer, and Resend delivers it
 * through the real Server Action under the operation id it was first sent with
 * — so the daemon answers the first admission's event instead of running it
 * twice. Proven in the same page, and after a reload. */
import type { Page, Request, Route } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { v7 } from "uuid";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLEET_PREFIX = "resend-spec-";
const CHAT_LABEL = "Fleet chat";
const TRANSCRIPT_LABEL = "Chat";
const COMPOSER_LABEL = "Chat composer";
const NOTICES_LABEL = "Unsent messages";
const SEND_LABEL = "Send";
const RESEND_LABEL = "Resend";
const SEND_UNCONFIRMED = "Couldn't confirm this message was sent.";
const SERVER_ACTION_HEADER = "next-action";
const POST = "POST";
const UUID_V7 = /[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/;
// A logical event id as the daemon spells it: `<millis>-<seq>`.
const EVENT_ID = /\d{13}-\d+/;
const STORAGE_KEY_PREFIX = "agentsfleet:pending-sends";
const KEY_SEPARATOR = ":";
const STATE_SENDING = "sending";

type Steers = { lostAnswers: number; operationIds: string[]; eventIds: string[] };

type ResendPage = { workspaceId: string; fleetId: string; href: string; message: string };

// A seeded fleet, and every steer Server Action seen: each goes through to the
// daemon, and its operation id and the event id the daemon answered are read
// off the wire. With `loseFirstAnswer`, the first steer is admitted but its
// answer never reaches the page — the acknowledgement lost on the way back.
async function withResendPage(
  page: Page,
  loseFirstAnswer: boolean,
  body: (resend: ResendPage, steers: Steers) => Promise<void>,
): Promise<void> {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${FLEET_PREFIX}${crypto.randomUUID()}` });
  const href = workspaceHref(workspaceId, `fleets/${fleet.id}`);
  const message = `resend probe ${crypto.randomUUID().slice(0, 8)}`;
  const steers: Steers = { lostAnswers: 0, operationIds: [], eventIds: [] };
  await page.route((url) => url.pathname === href, async (route) => {
    const request = route.request();
    if (!isSteer(request, message)) return route.fallback();
    steers.operationIds.push(operationIdOf(request));
    return answerThrough(route, steers, loseFirstAnswer);
  });
  try {
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    await signInAs(page, FIXTURE_KEY.regular);
    await body({ workspaceId, fleetId: fleet.id, href, message }, steers);
  } finally {
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, FLEET_PREFIX);
  }
}

async function answerThrough(route: Route, steers: Steers, loseFirstAnswer: boolean): Promise<void> {
  const response = await route.fetch();
  steers.eventIds.push((await response.text()).match(EVENT_ID)?.[0] ?? "");
  if (loseFirstAnswer && steers.lostAnswers === 0) {
    steers.lostAnswers += 1;
    return route.abort("failed");
  }
  return route.fulfill({ response });
}

function isSteer(request: Request, message: string): boolean {
  return request.method() === POST
    && request.headers()[SERVER_ACTION_HEADER] !== undefined
    && (request.postData() ?? "").includes(message);
}

function operationIdOf(request: Request): string {
  return (request.postData() ?? "").match(UUID_V7)?.[0] ?? "";
}

function chatParts(page: Page) {
  const chat = page.getByLabel(CHAT_LABEL);
  const composer = page.getByLabel(COMPOSER_LABEL);
  return {
    chat,
    // The transcript alone: the composer's textarea mirrors its draft into its
    // own text, so the whole panel would match the restored draft.
    transcript: chat.getByRole("log", { name: TRANSCRIPT_LABEL }),
    composer,
    draft: composer.getByPlaceholder(/message this fleet/i),
    notices: composer.getByRole("list", { name: NOTICES_LABEL }),
  };
}

test("test_failed_send_resend_journey", async ({ page }) => {
  await withResendPage(page, true, async ({ href, message }, steers) => {
    await page.goto(href);
    const { chat, transcript, composer, draft, notices } = chatParts(page);
    await expect(chat).toBeVisible();

    await draft.fill(message);
    await composer.getByRole("button", { name: SEND_LABEL }).click();
    // The daemon took it, but the answer never arrived: the page cannot know,
    // so it says the send could not be confirmed and hands the text back.
    await expect(notices.getByText(SEND_UNCONFIRMED)).toBeVisible();
    await expect(draft).toHaveValue(message);

    await notices.getByRole("button", { name: RESEND_LABEL }).click();
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(1);
    await expect(composer.getByText(SEND_UNCONFIRMED)).toHaveCount(0);
    await expect(draft).toHaveValue("");
    // One operation, admitted once: Resend carried the first send's id, and
    // the daemon answered it with the event it had already admitted.
    const [firstOperation, resentOperation] = steers.operationIds;
    expect(firstOperation).toMatch(UUID_V7);
    expect(resentOperation).toBe(firstOperation);
    const [admitted, answered] = steers.eventIds;
    expect(admitted).toMatch(EVENT_ID);
    expect(answered).toBe(admitted);
  });
});

test("test_reload_recovers_unconfirmed_send", async ({ page }) => {
  await withResendPage(page, false, async ({ workspaceId, fleetId, href, message }, steers) => {
    // A send the last document made and never heard back on: the daemon holds
    // it, and its ledger entry is still `sending` in storage when this document
    // loads.
    const operationId = v7();
    const admitted = await clientFor(FIXTURE_KEY.regular).post<{ event_id: string }>(
      `/v1/workspaces/${workspaceId}/fleets/${fleetId}/messages`,
      { message, operation_id: operationId },
    );
    await page.goto(href);
    // The ledger is keyed by the signed-in user, so the seeded entry is too.
    await page.waitForFunction(() => Boolean(window.Clerk?.user?.id));
    const subject = await page.evaluate(() => window.Clerk?.user?.id ?? "");
    const key = [STORAGE_KEY_PREFIX, subject, workspaceId, fleetId].join(KEY_SEPARATOR);
    const entry = [{ operationId, text: message, state: STATE_SENDING, submittedAtMs: Date.now() }];
    await page.evaluate(({ storageKey, value }) => window.localStorage.setItem(storageKey, value), {
      storageKey: key,
      value: JSON.stringify(entry),
    });
    await page.reload();

    const { chat, transcript, notices } = chatParts(page);
    await expect(chat).toBeVisible();
    await expect(notices.getByText(SEND_UNCONFIRMED)).toBeVisible();
    await expect(notices.getByText(message)).toBeVisible();

    await notices.getByRole("button", { name: RESEND_LABEL }).click();
    await expect(notices).toHaveCount(0);
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(1);
    expect(steers.operationIds).toEqual([operationId]);
    expect(steers.eventIds).toEqual([admitted.event_id]);
  });
});
