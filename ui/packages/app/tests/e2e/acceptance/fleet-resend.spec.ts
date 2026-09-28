/** A send that did not come back, the Claude.ai way: the row leaves the
 * thread, the text returns to the composer, and Resend delivers it once
 * through the real Server Action under the operation id it was first sent
 * with — in the same page, and after a reload. */
import type { Page, Request } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { v7 } from "uuid";
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
const SEND_UNCONFIRMED = "Couldn't confirm this message was sent.";
const SERVER_ACTION_HEADER = "next-action";
const POST = "POST";
const UUID_V7 = /[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/;
const STORAGE_KEY_PREFIX = "agentsfleet:pending-sends";
const STATE_SENDING = "sending";

type Steers = { refused: number; delivered: number; operationIds: string[] };

type ResendPage = { workspaceId: string; fleetId: string; href: string; message: string };

// A seeded fleet, and its steer Server Actions counted: the page fires other
// actions on load, so a steer is recognised by the text it carries. Each
// steer's operation id is read off the action's arguments.
async function withResendPage(
  page: Page,
  refuseFirst: boolean,
  body: (resend: ResendPage, steers: Steers) => Promise<void>,
): Promise<void> {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${FLEET_PREFIX}${crypto.randomUUID()}` });
  const href = workspaceHref(workspaceId, `fleets/${fleet.id}`);
  const message = `resend probe ${crypto.randomUUID().slice(0, 8)}`;
  const steers: Steers = { refused: 0, delivered: 0, operationIds: [] };
  await page.route((url) => url.pathname === href, async (route) => {
    const request = route.request();
    if (!isSteer(request, message)) return route.fallback();
    steers.operationIds.push(operationIdOf(request));
    if (refuseFirst && steers.refused === 0) {
      steers.refused += 1;
      return route.abort("failed");
    }
    steers.delivered += 1;
    return route.fallback();
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
    await composer.getByRole("button", { name: "Send" }).click();
    // The request died on the wire, so nothing answered: the notice says the
    // send could not be confirmed, not that it was refused.
    await expect(notices.getByText(SEND_UNCONFIRMED)).toBeVisible();
    // The unsent text is back where it was typed, and nowhere in the thread.
    await expect(draft).toHaveValue(message);
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(0);

    await notices.getByRole("button", { name: "Resend" }).click();
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(1);
    await expect(composer.getByText(SEND_UNCONFIRMED)).toHaveCount(0);
    await expect(draft).toHaveValue("");
    expect({ refused: steers.refused, delivered: steers.delivered }).toEqual({ refused: 1, delivered: 1 });
    // One operation, named once: Resend carried the id the first send had.
    const [first, second] = steers.operationIds;
    expect(first).toMatch(UUID_V7);
    expect(second).toBe(first);
  });
});

test("test_reload_recovers_unconfirmed_send", async ({ page }) => {
  await withResendPage(page, false, async ({ workspaceId, fleetId, href, message }, steers) => {
    // A send the last document made and never heard back on: its ledger entry
    // is still `sending` in storage when this document loads.
    const operationId = v7();
    const key = [STORAGE_KEY_PREFIX, workspaceId, fleetId].join(":");
    const entry = [{ operationId, text: message, state: STATE_SENDING, submittedAtMs: Date.now() }];
    await page.addInitScript(({ storageKey, value }) => {
      window.localStorage.setItem(storageKey, value);
    }, { storageKey: key, value: JSON.stringify(entry) });

    await page.goto(href);
    const { chat, transcript, notices } = chatParts(page);
    await expect(chat).toBeVisible();
    await expect(notices.getByText(SEND_UNCONFIRMED)).toBeVisible();
    await expect(notices.getByText(message)).toBeVisible();

    await notices.getByRole("button", { name: "Resend" }).click();
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(1);
    await expect(notices).toHaveCount(0);
    expect(steers.operationIds).toEqual([operationId]);
  });
});
