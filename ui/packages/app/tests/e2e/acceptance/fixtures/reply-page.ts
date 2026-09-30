/** A seeded fleet's chat whose live stream the test writes; history reads
 * empty, so every row on screen came from the frames a test sends. */
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { EventsPage } from "@/lib/api/events";
import { ACTOR } from "@/lib/events/event-summary";
import { fixtureSubject, signInAs } from "./auth";
import { FIXTURE_KEY } from "./constants";
import { workspaceHref } from "./nav";
import { pageEventStream, type ScheduledStream } from "./page-event-stream";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./seed";
import { cleanWorkspaceFleets } from "./teardown";

const EMPTY_HISTORY: EventsPage = { items: [], next_cursor: null };
const CHAT_LABEL = "Fleet chat";

/** `ownActor` is the signed-in operator's steer actor: a turn under it is the
 * viewer's own, the one the thread anchors while its reply runs. */
export type ReplyPage = { chat: Locator; stream: ScheduledStream; ownActor: string };

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
    await body({ chat, stream, ownActor: `${ACTOR.STEER_PREFIX}${fixtureSubject(FIXTURE_KEY.regular)}` });
    expect(errors).toEqual([]);
  } finally {
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, fleetPrefix);
  }
}
