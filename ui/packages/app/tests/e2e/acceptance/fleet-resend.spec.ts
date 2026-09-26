/** A refused send, the Claude.ai way: the row leaves the thread, the text
 * returns to the composer, and Resend delivers it once through the real
 * Server Action. */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLEET_PREFIX = "resend-spec-";
const CHAT_LABEL = "Fleet chat";
const TRANSCRIPT_LABEL = "Chat";
const COMPOSER_LABEL = "Chat composer";
const SEND_FAILED = "Message not sent.";
const SERVER_ACTION_HEADER = "next-action";
const POST = "POST";

test("test_failed_send_resend_journey", async ({ page }) => {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${FLEET_PREFIX}${crypto.randomUUID()}` });
  const href = workspaceHref(workspaceId, `fleets/${fleet.id}`);
  const message = `resend probe ${crypto.randomUUID().slice(0, 8)}`;
  const actions = { refused: 0, delivered: 0 };
  // The first steer POST fails at the network, the way an offline send does;
  // the Resend goes through to the real Server Action. The page fires other
  // actions on load, so the steer is recognised by the text it carries.
  await page.route((url) => url.pathname === href, async (route) => {
    const request = route.request();
    const isSteer = request.method() === POST
      && request.headers()[SERVER_ACTION_HEADER] !== undefined
      && (request.postData() ?? "").includes(message);
    if (!isSteer) return route.fallback();
    if (actions.refused === 0) {
      actions.refused += 1;
      return route.abort("failed");
    }
    actions.delivered += 1;
    return route.fallback();
  });
  try {
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(href);
    const chat = page.getByLabel(CHAT_LABEL);
    // The transcript alone: the composer's textarea mirrors its draft into its
    // own text, so the whole panel would match the restored draft.
    const transcript = chat.getByRole("log", { name: TRANSCRIPT_LABEL });
    const composer = page.getByLabel(COMPOSER_LABEL);
    const draft = composer.getByPlaceholder(/message this fleet/i);
    await expect(chat).toBeVisible();

    await draft.fill(message);
    await composer.getByRole("button", { name: "Send" }).click();
    await expect(composer.getByText(SEND_FAILED)).toBeVisible();
    // The refused text is back where it was typed, and nowhere in the thread.
    await expect(draft).toHaveValue(message);
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(0);

    await composer.getByRole("button", { name: "Resend" }).click();
    await expect(transcript.getByText(message, { exact: true })).toHaveCount(1);
    await expect(composer.getByText(SEND_FAILED)).toHaveCount(0);
    await expect(draft).toHaveValue("");
    expect(actions).toEqual({ refused: 1, delivered: 1 });
  } finally {
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, FLEET_PREFIX);
  }
});
