/** The thread holds the reader's place: a Thought folding, a reply settling,
 * a send, another sender's turn, the operator's own turn from another tab and
 * the offline notice each leave the view where it was. Frames go into the
 * page's own EventSource, so a reply can be held mid-run, which is when the
 * thread anchors a turn this tab sent and keeps room below it. */
import type { Locator } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { FRAME_KIND } from "@/lib/api/events-types";
import type { ScheduledStream, TimedFrame } from "./fixtures/page-event-stream";
import { withReplyPage } from "./fixtures/reply-page";
import { sseFrame as frame } from "./fixtures/sse";

const FLEET_PREFIX = "thread-anchor-spec-";
const EVENT_ID = "9300000000000-1";
// Another operator in the workspace: a turn this tab did not send.
const TEAMMATE = "steer:user_thread_anchor_teammate";
const COMPOSER_LABEL = "Chat composer";
const SEND_LABEL = "Send";
const JUMP_TO_LATEST = "Jump to latest";
const THOUGHT = /^Thought · /;
// Long enough that the thought opens (FleetThought opens after 400 ms live).
const REASONING_CHUNKS = 12;
const REASONING_EVERY_MS = 80;
const ANSWER = "Checked every signature; nothing to change.";
const HISTORY_TURNS = 30;
const HISTORY_ID_BASE = 9_200_000_000_000;
// The library's reserve trails a 200 ms fold, and the re-pin when a send's
// rows take their admitted ids, by a frame or a few: the view may wobble by
// about a line, and must come back to where it was.
const WOBBLE_PX = 40;
const SETTLED_PX = 1;
const SAMPLE_MS = 450;
const SEND_SAMPLE_MS = 1_500;
// Frames of unchanged scroll height that count as a thread at rest.
const STILL_FRAMES = 10;
const SENT = "anchor probe";
// The turn the fold and settle journeys run: sent here, so the thread anchors it.
const OWN_TURN = "check every signature";
const NOTICE = "fleet-connection-notice";
const HEADER = "fleet-chat-header";
// Six failed connects report the stream offline: five fast attempts backing off
// 2, 4, 8, 15 and 15 s, each jittered down to half (fleet-stream-reconnect.ts).
const OFFLINE_WITHIN_MS = 60_000;
const OFFLINE_TEST_MS = 120_000;

test("test_fold_at_bottom_holds_view", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream, ownActor, sendOwn }) => {
    await sendOwn(OWN_TURN, EVENT_ID);
    await stream.send(liveReply(Date.now(), ownActor));
    await stream.send([answer(), complete(ownActor)]);
    const thought = chat.getByRole("button", { name: THOUGHT });
    await expect(thought).toBeVisible();
    await thought.click();
    await expect(thought).toHaveAttribute("aria-expanded", "true");
    await page.waitForTimeout(SAMPLE_MS);
    const tops = await sampleTop(thought, SAMPLE_MS, () => thought.click());
    await test.info().attach("fold-trigger-tops", { contentType: "application/json", body: JSON.stringify(tops.map(Math.round)) });
    expectHeld(tops);
  });
});

test("test_settle_holds_view", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream, ownActor, sendOwn }) => {
    await sendOwn(OWN_TURN, EVENT_ID);
    await stream.send(liveReply(Date.now(), ownActor));
    await expect(chat.getByRole("button", { name: /^Thinking/ })).toHaveAttribute("aria-expanded", "true");
    // The reply's own row, under the operator's bubble: the Thought folds
    // inside it, below its top.
    const replyRow = chat.locator('[data-role="assistant"]').last();
    const tops = await sampleTop(replyRow, SAMPLE_MS, () => stream.send([answer(), complete(ownActor)]));
    expectHeld(tops);
    await expect(replyRow.getByText(ANSWER)).toBeVisible();
  });
});

test("test_send_scrolls_once", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream }) => {
    await stream.send(history(Date.now() - HISTORY_TURNS));
    await expect(chat.getByText(`Settled answer ${HISTORY_TURNS}`)).toBeVisible();
    const composer = page.getByLabel(COMPOSER_LABEL);
    await composer.getByRole("textbox").fill(SENT);
    const viewport = chat.locator('[role="presentation"]');
    await restAtBottom(viewport);
    const tops = await sampleScroll(viewport, SEND_SAMPLE_MS, () =>
      composer.getByRole("button", { name: SEND_LABEL, exact: true }).click(),
    );
    await test.info().attach("send-scroll-tops", { contentType: "application/json", body: JSON.stringify(tops.map(Math.round)) });
    // One way: never further back than a wobble behind the furthest point
    // reached, and resting at that point once the send is admitted.
    const furthest = tops.map((_, i) => Math.max(...tops.slice(0, i + 1)));
    expect(Math.max(...tops.map((top, i) => (furthest[i] ?? top) - top))).toBeLessThanOrEqual(WOBBLE_PX);
    expect(Math.abs((tops.at(-1) ?? 0) - Math.max(...tops))).toBeLessThanOrEqual(SETTLED_PX);
    await expect(chat.getByText(SENT)).toBeInViewport();
  });
});

test("test_jump_to_latest_after_anchor", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream }) => {
    await readingHistory(chat, stream);
    await stream.send([opening(Date.now(), TEAMMATE), answer(), complete(TEAMMATE)]);
    await chat.getByRole("button", { name: JUMP_TO_LATEST }).click();
    await expect(chat.getByText(ANSWER, { exact: true })).toBeInViewport();
  });
});

// A turn this tab did not send, from a teammate or the fleet's own API,
// leaves a reader in the history where they are.
test("test_background_turn_leaves_history_alone", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream }) => {
    await expectHistoryHeld(chat, stream, TEAMMATE, "background-turn-tops");
  });
});

// The operator's own account sending from another tab reaches this one by
// frame alone: this tab's reader stays in the history too.
test("test_other_tab_turn_leaves_history_alone", async ({ page }) => {
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream, ownActor }) => {
    await expectHistoryHeld(chat, stream, ownActor, "other-tab-turn-tops");
  });
});

// The notice arrives over a reader back in the history. Measured on screen:
// the notice once sat in the panel above the thread and pushed it down.
test("test_reconnect_notice_warns_in_place", async ({ page }) => {
  test.setTimeout(OFFLINE_TEST_MS);
  await withReplyPage(page, FLEET_PREFIX, async ({ chat, stream }) => {
    await readingHistory(chat, stream);
    const reading = chat.getByText("Settled answer 1", { exact: true });
    await stream.drop();
    // The header row names the lost connection first: page chrome, not the
    // notice, and not what this measures.
    await expect(page.getByTestId(HEADER)).toBeVisible();
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const before = await reading.evaluate((el) => el.getBoundingClientRect().top);
    const notice = page.getByTestId(NOTICE);
    await expect(notice).toBeVisible({ timeout: OFFLINE_WITHIN_MS });
    const after = await reading.evaluate((el) => el.getBoundingClientRect().top);
    expect(Math.abs(after - before)).toBeLessThanOrEqual(SETTLED_PX);
    await expect(notice).toHaveAttribute("role", "alert");
    await expect(notice).toHaveClass(/\btext-warning\b/);
    // At the bottom, the newest row stays above the notice, never behind it.
    await restAtBottom(chat.locator('[role="presentation"]'));
    const newest = chat.getByText(`Settled answer ${HISTORY_TURNS}`, { exact: true });
    const rowBottom = await newest.evaluate((el) => el.getBoundingClientRect().bottom);
    const noticeTop = await notice.evaluate((el) => el.getBoundingClientRect().top);
    expect(rowBottom).toBeLessThanOrEqual(noticeTop);
  });
});

// A reader back at the first settled turn while `actor`'s reply starts
// running: the turn they are reading never moves.
async function expectHistoryHeld(chat: Locator, stream: ScheduledStream, actor: string, attachment: string): Promise<void> {
  await readingHistory(chat, stream);
  const reading = chat.getByText("Settled answer 1", { exact: true });
  const tops = await sampleTop(reading, SEND_SAMPLE_MS, () => stream.send(liveReply(Date.now(), actor)));
  await test.info().attach(attachment, { contentType: "application/json", body: JSON.stringify(tops.map(Math.round)) });
  expect(Math.max(...tops.map((top) => Math.abs(top - (tops[0] ?? top))))).toBeLessThanOrEqual(SETTLED_PX);
  await expect(chat.getByRole("button", { name: JUMP_TO_LATEST })).toBeVisible();
}

// Thirty settled turns, the reader scrolled back to the first: the Jump to
// latest control shows once the thread knows the reader left the bottom.
async function readingHistory(chat: Locator, stream: ScheduledStream): Promise<void> {
  await stream.send(history(Date.now() - HISTORY_TURNS));
  await expect(chat.getByText(`Settled answer ${HISTORY_TURNS}`)).toBeVisible();
  const viewport = chat.locator('[role="presentation"]');
  await restAtBottom(viewport);
  await chat.getByText("Settled answer 1", { exact: true }).scrollIntoViewIfNeeded();
  await expect(chat.getByRole("button", { name: JUMP_TO_LATEST })).toBeVisible();
}

// The element's top within the thread's scroll viewport, on every frame while
// `act` runs and after. Measured against the viewport's own box, so the page
// chrome around the thread moving it (a header row going) is not counted.
async function sampleTop(target: Locator, ms: number, act: () => Promise<unknown>): Promise<number[]> {
  const sampling = target.evaluate((el, window_) => new Promise<number[]>((resolve) => {
    const tops: number[] = [];
    const frame = el.closest('[role="presentation"]') ?? document.documentElement;
    const t0 = performance.now();
    const tick = () => {
      tops.push(el.getBoundingClientRect().top - frame.getBoundingClientRect().top);
      if (performance.now() - t0 < window_) requestAnimationFrame(tick);
      else resolve(tops);
    };
    requestAnimationFrame(tick);
  }), ms);
  await act();
  return sampling;
}

async function sampleScroll(viewport: Locator, ms: number, act: () => Promise<unknown>): Promise<number[]> {
  const sampling = viewport.evaluate((el, window_) => new Promise<number[]>((resolve) => {
    const tops: number[] = [];
    const t0 = performance.now();
    const tick = () => {
      tops.push(el.scrollTop);
      if (performance.now() - t0 < window_) requestAnimationFrame(tick);
      else resolve(tops);
    };
    requestAnimationFrame(tick);
  }), ms);
  await act();
  return sampling;
}

// A reader who has been at the bottom sees settled rows at their real height;
// a jump there first lays out the rows it lands on, one frame at a time.
async function restAtBottom(viewport: Locator): Promise<void> {
  await viewport.evaluate((el, frames) => new Promise<void>((resolve) => {
    let still = 0;
    let height = -1;
    const tick = () => {
      el.scrollTo({ top: el.scrollHeight });
      still = el.scrollHeight === height ? still + 1 : 0;
      height = el.scrollHeight;
      if (still >= frames) resolve();
      else requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }), STILL_FRAMES);
}

function expectHeld(tops: number[]): void {
  const start = tops[0] ?? 0;
  const drift = Math.max(...tops.map((top) => Math.abs(top - start)));
  expect(drift).toBeLessThanOrEqual(WOBBLE_PX);
  expect(Math.abs((tops.at(-1) ?? start) - start)).toBeLessThanOrEqual(SETTLED_PX);
}

function opening(createdAt: number, actor: string): TimedFrame {
  return { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: EVENT_ID, actor, created_at: createdAt }) };
}

function liveReply(createdAt: number, actor: string): TimedFrame[] {
  const reasoning = Array.from({ length: REASONING_CHUNKS }, (_, index) => ({
    afterMs: REASONING_EVERY_MS,
    body: chunkBody(index, "reasoning", `Weighing signature ${index + 1} against the expected key before trusting the delivery. `),
  }));
  return [opening(createdAt, actor), ...reasoning];
}

function answer(): TimedFrame {
  return { afterMs: 0, body: chunkBody(REASONING_CHUNKS, "answer", ANSWER) };
}

function complete(actor: string): TimedFrame {
  return {
    afterMs: 0,
    body: frame(FRAME_KIND.EVENT_COMPLETE, { event_id: EVENT_ID, actor, status: "processed", final_reply: ANSWER }),
  };
}

function chunkBody(seq: number, kind: "reasoning" | "answer", text: string): string {
  return frame(FRAME_KIND.CHUNK, {
    event_id: EVENT_ID, text, text_kind: kind,
    stream_seq: seq, stream_start: seq === 0, stream_contiguous: true,
  });
}

function history(firstCreatedAt: number): TimedFrame[] {
  return Array.from({ length: HISTORY_TURNS }, (_, index) => {
    const eventId = `${HISTORY_ID_BASE + index}-1`;
    const createdAt = firstCreatedAt + index;
    return [
      { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: eventId, actor: TEAMMATE, created_at: createdAt }) },
      {
        afterMs: 0,
        body: frame(FRAME_KIND.EVENT_COMPLETE, {
          event_id: eventId, actor: TEAMMATE, created_at: createdAt, status: "processed",
          final_reply: `Settled answer ${index + 1}`,
        }),
      },
    ];
  }).flat();
}
