/** A reply streamed over time into the real page, frame by frame through the
 * page's own EventSource: the Thought chip live then folded, a timed tool row,
 * and what the reply costs the main thread while it streams. */
import type { Locator, Page } from "@playwright/test";
import type { EventsPage } from "@/lib/api/events";
import { expect, test } from "@playwright/test";
import { FRAME_KIND } from "@/lib/api/events-types";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { sseFrame as frame } from "./fixtures/sse";
import { pageEventStream, type ScheduledStream, type TimedFrame } from "./fixtures/page-event-stream";

const FLEET_PREFIX = "reply-parts-spec-";
const EVENT_ID = "9200000000000-1";
const ACTOR = "steer:reply-parts@agentsfleet.dev";
const REASONING_CHUNKS = 60;
const REASONING_EVERY_MS = 50;
const ANSWER_CHUNKS = 60;
const ANSWER_EVERY_MS = 40;
const LAST_ANSWER_LINE = `Step ${ANSWER_CHUNKS} settled`;
const TOOL_NAME = "read_file";
const TOOL_WALL_MS = 700;
const LIVE_REASONING = "Checking whether delivery 1 is signed before trusting it.";
const ANSWER = "Signed by the expected key.";
// The pre-change reply measured 16.8 ms p95 on this lane (baseline run on
// 5f236cbcc); the budget is the one PR #717 recorded before that.
const FRAME_P95_BUDGET_MS = 17.6;
const P95 = 0.95;
const PROBE_KEY = "__replyFrameProbe";
// The settled turns above the streaming reply while it is measured. Each is an
// operator turn, so each renders as two messages — the history a streaming
// frame must not re-convert.
const HISTORY_TURNS = 100;
const HISTORY_ID_BASE = 9_100_000_000_000;
const HISTORY_STATUS = "processed";
const LAST_HISTORY_ANSWER = `Settled answer ${HISTORY_TURNS}`;
const EMPTY_HISTORY: EventsPage = { items: [], next_cursor: null };
const COPY_REPLY = "Copy reply";
// How far the shared focus ring reaches past a control: 2px wide on a 2px offset.
const RING_REACH_PX = 4;
// One settled turn is its opening frame and its completion.
const FRAMES_PER_TURN = 2;

type ProbeResult = { longTasks: number; frames: number; p95FrameMs: number };
type ReplyPage = { chat: Locator; stream: ScheduledStream };

test("test_stream_reply_parts_live_then_folded", async ({ page }) => {
  await withReplyPage(page, async ({ chat, stream }) => {
    let seq = 0;
    await stream.send([opening(Date.now()), chunk(seq++, "reasoning", LIVE_REASONING)]);
    const live = chat.getByRole("button", { name: /^Thinking/ });
    await expect(live).toBeVisible();
    await expect(live).toContainText(LIVE_REASONING.slice(0, -1));

    await stream.send([toolFrame(FRAME_KIND.TOOL_CALL_STARTED, { args_redacted: {} })]);
    const tool = chat.getByRole("list", { name: "Tool calls" }).locator(`[data-tool="${TOOL_NAME}"]`);
    await expect(tool).toBeVisible();
    await stream.send([toolFrame(FRAME_KIND.TOOL_CALL_COMPLETED, { ms: TOOL_WALL_MS })]);
    await expect(tool).toHaveAttribute("data-done", "true");
    // Pin test: the wall time the frame reported, as the row prints it.
    await expect(tool).toContainText("0.7s");

    await stream.send([chunk(seq++, "answer", ANSWER)]);
    await expect(chat.getByText(ANSWER)).toBeVisible();
    const folded = chat.getByRole("button", { name: /^Thought · \d+\.\ds$/ });
    await expect(folded).toBeVisible();
    await expect(chat.getByText(LIVE_REASONING)).toHaveCount(0);
    await folded.click();
    await expect(chat.getByText(LIVE_REASONING)).toBeVisible();
  });
});

test("test_streaming_reply_costs_no_long_tasks", async ({ page }, testInfo) => {
  await withReplyPage(page, async ({ chat, stream }) => {
    // A thread with a real history first, then the measured reply beneath it.
    const now = Date.now();
    await stream.send(settledHistory(now - HISTORY_TURNS));
    await expect(chat.getByText(LAST_HISTORY_ANSWER)).toBeVisible();
    await startFrameProbe(page);
    await stream.send(replySchedule(now));
    await expect(chat.getByText(LAST_ANSWER_LINE)).toBeVisible();
    const probe = await stopFrameProbe(page);
    await testInfo.attach("streaming-reply-frame-evidence", {
      contentType: "application/json",
      body: JSON.stringify({
        ...probe,
        budgetMs: FRAME_P95_BUDGET_MS,
        schedule: { HISTORY_TURNS, REASONING_CHUNKS, REASONING_EVERY_MS, ANSWER_CHUNKS, ANSWER_EVERY_MS },
      }),
    });
    expect(probe.longTasks).toBe(0);
    expect(probe.p95FrameMs).toBeLessThanOrEqual(FRAME_P95_BUDGET_MS);
  });
});

// A settled row skips layout off screen, which contains its paint; a control
// focused near its edge must still show the whole ring, not a clipped arc.
test("test_settled_row_keeps_its_focus_ring", async ({ page }) => {
  await withReplyPage(page, async ({ chat, stream }) => {
    await stream.send(settledHistory(Date.now()).slice(0, FRAMES_PER_TURN));
    const copy = chat.getByRole("button", { name: COPY_REPLY });
    await expect(copy).toBeVisible();
    // Onto Copy by keyboard, so its focus ring is the one a keyboard user sees.
    await copy.focus();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    await expect(copy).toBeFocused();

    // `has` runs inside each row, so its locator starts from the page, not
    // the chat: a chat-rooted one looks for "Fleet chat" within the row.
    const row = chat.locator('[data-settled="true"]').filter({ has: page.getByRole("button", { name: COPY_REPLY }) });
    const [control, painted] = await Promise.all([copy.boundingBox(), row.boundingBox()]);
    expect(control).not.toBeNull();
    expect(painted).not.toBeNull();
    if (control === null || painted === null) return;
    expect(control.x - RING_REACH_PX).toBeGreaterThanOrEqual(painted.x);
    expect(control.y - RING_REACH_PX).toBeGreaterThanOrEqual(painted.y);
    expect(control.x + control.width + RING_REACH_PX).toBeLessThanOrEqual(painted.x + painted.width);
    expect(control.y + control.height + RING_REACH_PX).toBeLessThanOrEqual(painted.y + painted.height);
  });
});

// A seeded fleet whose live stream the test writes; history reads empty, so
// every row on screen came from the frames a test sends.
async function withReplyPage(page: Page, body: (reply: ReplyPage) => Promise<void>): Promise<void> {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, { name: `${FLEET_PREFIX}${crypto.randomUUID()}` });
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
    const chat = page.getByLabel("Fleet chat");
    await expect(chat).toBeVisible();
    await stream.connected;
    await body({ chat, stream });
    expect(errors).toEqual([]);
  } finally {
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, FLEET_PREFIX);
  }
}

// Settled operator turns, each opened and completed with its answer inline, the
// way the daemon publishes a finished turn (`final_reply` on the completion).
function settledHistory(firstCreatedAt: number): TimedFrame[] {
  return Array.from({ length: HISTORY_TURNS }, (_, index) => {
    const eventId = `${HISTORY_ID_BASE + index}-1`;
    const createdAt = firstCreatedAt + index;
    return [
      { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: eventId, actor: ACTOR, created_at: createdAt }) },
      {
        afterMs: 0,
        body: frame(FRAME_KIND.EVENT_COMPLETE, {
          event_id: eventId, actor: ACTOR, created_at: createdAt, status: HISTORY_STATUS,
          final_reply: `Settled answer ${index + 1}`,
        }),
      },
    ];
  }).flat();
}

function opening(createdAt: number): TimedFrame {
  return { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: EVENT_ID, actor: ACTOR, created_at: createdAt }) };
}

// One reply as the runner streams it: typed chunks on one contiguous
// sequence, reasoning first, then a markdown answer the page parses as it grows.
function replySchedule(createdAt: number): TimedFrame[] {
  const reasoning = Array.from({ length: REASONING_CHUNKS }, (_, index) => ({
    ...chunk(index, "reasoning", `Checking whether delivery ${index + 1} is signed before trusting it. `),
    afterMs: REASONING_EVERY_MS,
  }));
  const answer = Array.from({ length: ANSWER_CHUNKS }, (_, index) => ({
    ...chunk(REASONING_CHUNKS + index, "answer", `- Step ${index + 1} settled with \`verify_signature\` and **no retries**\n`),
    afterMs: ANSWER_EVERY_MS,
  }));
  return [opening(createdAt), ...reasoning, ...answer];
}

function chunk(seq: number, kind: "reasoning" | "answer", text: string): TimedFrame {
  return {
    afterMs: 0,
    body: frame(FRAME_KIND.CHUNK, {
      event_id: EVENT_ID, text, text_kind: kind,
      stream_seq: seq, stream_start: seq === 0, stream_contiguous: true,
    }),
  };
}

function toolFrame(kind: string, extra: Record<string, unknown>): TimedFrame {
  return { afterMs: 0, body: frame(kind, { event_id: EVENT_ID, name: TOOL_NAME, ...extra }) };
}

// Long tasks from the browser's own observer; frame gaps from consecutive
// animation frames, so a slow render shows as a long frame even under 50 ms.
async function startFrameProbe(page: Page): Promise<void> {
  await page.evaluate((key) => {
    const probe = { longTasks: 0, gaps: [] as number[], running: true };
    new PerformanceObserver((list) => { probe.longTasks += list.getEntries().length; })
      .observe({ type: "longtask" });
    let last = performance.now();
    const tick = (now: number) => {
      probe.gaps.push(now - last);
      last = now;
      if (probe.running) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    (window as unknown as Record<string, typeof probe>)[key] = probe;
  }, PROBE_KEY);
}

async function stopFrameProbe(page: Page): Promise<ProbeResult> {
  return page.evaluate(({ key, p95 }) => {
    const probe = (window as unknown as Record<string, { longTasks: number; gaps: number[]; running: boolean }>)[key];
    if (probe === undefined) throw new Error("frame probe was never started");
    probe.running = false;
    const sorted = [...probe.gaps].sort((a, b) => a - b);
    const at = Math.max(0, Math.ceil(sorted.length * p95) - 1);
    return { longTasks: probe.longTasks, frames: sorted.length, p95FrameMs: sorted[at] ?? 0 };
  }, { key: PROBE_KEY, p95: P95 });
}
