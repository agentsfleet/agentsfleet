/** A reply streamed over time into the real page: reasoning, then a markdown
 * answer, delivered frame by frame from a local server. Measures what the
 * reply costs the main thread while it streams. */
import type { Page } from "@playwright/test";
import type { EventsPage } from "@/lib/api/events";
import { expect, test } from "@playwright/test";
import { FRAME_KIND } from "@/lib/api/events-types";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { sseFrame as frame } from "./fixtures/sse";
import { scheduledSseServer, type TimedFrame } from "./fixtures/sse-server";

const FLEET_PREFIX = "reply-parts-spec-";
const EVENT_ID = "9100000000000-1";
const ACTOR = "steer:reply-parts@agentsfleet.dev";
const REASONING_CHUNKS = 60;
const REASONING_EVERY_MS = 50;
const ANSWER_CHUNKS = 60;
const ANSWER_EVERY_MS = 40;
const LAST_ANSWER_LINE = `Step ${ANSWER_CHUNKS} settled`;
// The pre-change reply's measured p95 on this lane (PR #717 Session notes 2);
// re-measured on the pre-change tree before the parts rendering landed.
const FRAME_P95_BUDGET_MS = 17.6;
const P95 = 0.95;
const PROBE_KEY = "__replyFrameProbe";
const EMPTY_HISTORY: EventsPage = { items: [], next_cursor: null };

type ProbeResult = { longTasks: number; frames: number; p95FrameMs: number };

test("test_streaming_reply_costs_no_long_tasks", async ({ page }, testInfo) => {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
    name: `${FLEET_PREFIX}${crypto.randomUUID()}`,
  });
  const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleet.id}/events/stream`;
  const historyPath = streamPath.replace(/\/stream$/, "");
  const stream = await scheduledSseServer(replySchedule(Date.now()));
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route((url) => url.pathname === streamPath, (route) => route.continue({ url: stream.url }));
  await page.route((url) => url.pathname === historyPath, (route) => route.fulfill({ json: EMPTY_HISTORY }));
  try {
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`), { waitUntil: "domcontentloaded" });
    const chat = page.getByLabel("Fleet chat");
    await expect(chat).toBeVisible();
    await stream.connected;

    await startFrameProbe(page);
    await stream.play();
    await expect(chat.getByText(LAST_ANSWER_LINE)).toBeVisible();
    const probe = await stopFrameProbe(page);

    await testInfo.attach("streaming-reply-frame-evidence", {
      contentType: "application/json",
      body: JSON.stringify({
        ...probe,
        budgetMs: FRAME_P95_BUDGET_MS,
        schedule: { REASONING_CHUNKS, REASONING_EVERY_MS, ANSWER_CHUNKS, ANSWER_EVERY_MS },
      }),
    });
    expect(errors).toEqual([]);
    expect(probe.longTasks).toBe(0);
    expect(probe.p95FrameMs).toBeLessThanOrEqual(FRAME_P95_BUDGET_MS);
  } finally {
    await stream.close();
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, FLEET_PREFIX);
  }
});

// One reply as the runner streams it: typed chunks on one contiguous
// sequence, reasoning first, then a markdown answer the page parses as it grows.
function replySchedule(createdAt: number): TimedFrame[] {
  const opening = { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: EVENT_ID, actor: ACTOR, created_at: createdAt }) };
  const reasoning = Array.from({ length: REASONING_CHUNKS }, (_, index) => ({
    afterMs: REASONING_EVERY_MS,
    body: chunk(index, "reasoning", `Checking whether delivery ${index + 1} is signed before trusting it. `),
  }));
  const answer = Array.from({ length: ANSWER_CHUNKS }, (_, index) => ({
    afterMs: ANSWER_EVERY_MS,
    body: chunk(REASONING_CHUNKS + index, "answer", `- Step ${index + 1} settled with \`verify_signature\` and **no retries**\n`),
  }));
  return [opening, ...reasoning, ...answer];
}

function chunk(seq: number, kind: "reasoning" | "answer", text: string): string {
  return frame(FRAME_KIND.CHUNK, {
    event_id: EVENT_ID, text, text_kind: kind,
    stream_seq: seq, stream_start: seq === 0, stream_contiguous: true,
  });
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
