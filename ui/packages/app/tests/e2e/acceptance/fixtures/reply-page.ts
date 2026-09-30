/** A seeded fleet's chat whose live stream the test writes; history reads
 * empty, so every row on screen came from the frames a test sends or a send
 * it made from the composer. */
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { EventsPage } from "@/lib/api/events";
import { steerMessagesUrl, type SteerAccepted } from "@/lib/api/fleets-types";
import { ACTOR } from "@/lib/events/event-summary";
import { fixtureSubject, signInAs } from "./auth";
import { FIXTURE_KEY } from "./constants";
import { workspaceHref } from "./nav";
import { pageEventStream, type ScheduledStream } from "./page-event-stream";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./seed";
import { cleanWorkspaceFleets } from "./teardown";

const EMPTY_HISTORY: EventsPage = { items: [], next_cursor: null };
const CHAT_LABEL = "Fleet chat";
const TRANSCRIPT_LABEL = "Chat";
const COMPOSER_LABEL = "Chat composer";
const SEND_LABEL = "Send";
const HTTP_ACCEPTED = 202;
const STEER_ACCEPTED = "accepted";

/** `ownActor` is the signed-in operator's steer actor. `sendOwn` sends from
 * this tab's composer and answers the steer with a 202 naming `eventId`, so
 * the frames a test writes under `ownActor` next land on a turn this tab sent:
 * the one the thread anchors while its reply runs. The same actor arriving by
 * frame alone is the operator's turn from another tab. */
export type ReplyPage = {
  chat: Locator;
  stream: ScheduledStream;
  ownActor: string;
  sendOwn: (text: string, eventId: string) => Promise<void>;
};

export async function withReplyPage(
  page: Page,
  fleetPrefix: string,
  body: (reply: ReplyPage) => Promise<void>,
): Promise<void> {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${fleetPrefix}${crypto.randomUUID()}` });
  const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleet.id}/events/stream`;
  const historyPath = streamPath.replace(/\/stream$/, "");
  const stream = await pageEventStream(page, streamPath);
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route((url) => url.pathname === historyPath, (route) => route.fulfill({ json: EMPTY_HISTORY }));
  try {
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`), { waitUntil: "domcontentloaded" });
    const chat = page.getByLabel(CHAT_LABEL);
    await expect(chat).toBeVisible();
    await stream.connected;
    const ownActor = `${ACTOR.STEER_PREFIX}${fixtureSubject(FIXTURE_KEY.regular)}`;
    const sendOwn = (text: string, eventId: string) => sendFromComposer(page, steerMessagesUrl(workspaceId, fleet.id), text, eventId);
    await body({ chat, stream, ownActor, sendOwn });
    expect(errors).toEqual([]);
  } finally {
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, fleetPrefix);
  }
}

// The steer never reaches the daemon: the test writes every frame of the turn,
// so the 202 is answered here and resolves once the page has been given it.
async function sendFromComposer(page: Page, steerPath: string, text: string, eventId: string): Promise<void> {
  const admitted = Promise.withResolvers<void>();
  const receipt: SteerAccepted = { status: STEER_ACCEPTED, event_id: eventId, replayed: false };
  await page.route((url) => url.pathname === steerPath, async (route) => {
    await route.fulfill({ status: HTTP_ACCEPTED, json: receipt });
    admitted.resolve();
  }, { times: 1 });
  const composer = page.getByLabel(COMPOSER_LABEL);
  await composer.getByRole("textbox").fill(text);
  await composer.getByRole("button", { name: SEND_LABEL, exact: true }).click();
  await admitted.promise;
  // The transcript alone: the composer mirrors its draft into its own text.
  const transcript = page.getByLabel(CHAT_LABEL).getByRole("log", { name: TRANSCRIPT_LABEL });
  await expect(transcript.getByText(text, { exact: true })).toBeVisible();
}
