/** A reply streamed over time into the real page, frame by frame through the
 * page's own EventSource: the Thought chip live then folded, a timed tool row,
 * and what the reply costs the main thread while it streams. */
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FIXTURE_KEY, FRAME_BUDGET_TAG } from "./fixtures/constants";
import { sseFrame as frame } from "./fixtures/sse";
import type { TimedFrame } from "./fixtures/page-event-stream";
import { withReplyPage } from "./fixtures/reply-page";

const FLEET_NAME = "pull-request-review";
const EVENT_ID = "9200000000000-1";
const ACTOR = "steer:reply-parts@agentsfleet.dev";
const REASONING_CHUNKS = 60;
const REASONING_EVERY_MS = 50;
const ANSWER_CHUNKS = 60;
const ANSWER_EVERY_MS = 40;
const LAST_ANSWER_LINE = `Step ${ANSWER_CHUNKS} settled`;
const TOOL_NAME = "file_read";
const TOOL_PATH = "src/probe.ts";
const TOOL_CALL_ID = "f0:0";
const TOOL_CALLS_LABEL = "Ran commands";
const INPUT_LABEL = "Input";
const OUTPUT_LABEL = "Output";
const RUNNING_LABEL = "Running";
const DONE_LABEL = "Success";
const TOOL_WALL_MS = 700;
const REQUEST_TOOL = "http_request";
const REQUEST_URL = "https://status.example.test/health";
const REQUEST_CALL_ID = "f1:0";
const REQUEST_OUTPUT = "200 OK\n{\"ok\":true}\n";
const SHIMMER = "tool-shimmer";
const LIVE_REASONING = "Checking whether delivery 1 is signed before trusting it.";
const INTERLEAVED_CHUNKS = 6;
const INTERLEAVED_SAMPLE_MS = 2_500;
const INTERLEAVED_ANSWER = "Hey! How can I help?";
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
const COPY_REPLY = "Copy reply";
// How far the shared focus ring reaches past a control: 2px wide on a 2px offset.
const RING_REACH_PX = 4;
// One settled turn is its opening frame and its completion.
const FRAMES_PER_TURN = 2;

type ProbeResult = { longTasks: number; frames: number; p95FrameMs: number };

test("test_stream_reply_parts_live_then_folded", async ({ page }) => {
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream }) => {
    let seq = 0;
    await stream.send([opening(Date.now()), chunk(seq++, "reasoning", LIVE_REASONING)]);
    const live = chat.getByRole("button", { name: /^Thinking/ });
    await expect(live).toBeVisible();
    await expect(live).toContainText(LIVE_REASONING.slice(0, -1));

    await stream.send([toolFrame(FRAME_KIND.TOOL_CALL_STARTED, { call_id: TOOL_CALL_ID, args_redacted: { path: TOOL_PATH } })]);
    await chat.getByRole("button", {name: TOOL_CALLS_LABEL, exact: true}).click();
    const call = chat.getByRole("list", {name: TOOL_CALLS_LABEL}).locator(`[data-tool="${TOOL_NAME}"]`);
    await expect(call).toContainText(RUNNING_LABEL);
    await call.getByRole("button", {name: /^(Ran|Running) command /}).click();
    await expect(call.getByRole("heading", {name: INPUT_LABEL, exact: true})).toBeVisible();
    await expect(call).toContainText(TOOL_PATH);
    await stream.send([toolFrame(FRAME_KIND.TOOL_CALL_COMPLETED, { call_id: TOOL_CALL_ID, ms: TOOL_WALL_MS, status: "succeeded" })]);
    await expect(call).toContainText(DONE_LABEL);
    await expect(call.getByText(RUNNING_LABEL, {exact: true})).toHaveCount(0);

    await stream.send([chunk(seq++, "answer", ANSWER)]);
    await expect(chat.getByText(ANSWER)).toBeVisible();
    const folded = chat.getByRole("button", { name: /^Thought · \d+\.\ds$/ });
    await expect(folded).toBeVisible();
    await expect(chat.getByText(LIVE_REASONING)).toHaveCount(0);
    await folded.click();
    await expect(chat.getByText(LIVE_REASONING)).toBeVisible();
  });
});

// A call as Codex draws it, live: the bullet breathes while the request runs,
// then settles green with its recorded name, outcome and expandable output.
// The animation is read from the browser's computed style, which no unit lane
// can see.
test("test_live_tool_cell_settles_green", async ({ page }) => {
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream }) => {
    await stream.send([
      opening(Date.now()),
      callFrame(REQUEST_TOOL, FRAME_KIND.TOOL_CALL_STARTED, { call_id: REQUEST_CALL_ID, args_redacted: { method: "GET", url: REQUEST_URL } }),
    ]);
    await chat.getByRole("button", {name: TOOL_CALLS_LABEL, exact: true}).click();
    const cell = chat.getByRole("list", { name: TOOL_CALLS_LABEL }).locator(`[data-tool="${REQUEST_TOOL}"]`);
    const bullet = cell.locator("[data-tool-shimmer]");
    await expect(cell).toContainText(RUNNING_LABEL);
    await cell.getByRole("button", {name: /^(Ran|Running) command /}).click();
    await expect(cell.getByRole("heading", {name: INPUT_LABEL, exact: true})).toBeVisible();
    await expect(cell).toContainText(REQUEST_URL);
    await expect(cell.getByRole("heading", {name: OUTPUT_LABEL, exact: true})).toHaveCount(0);
    await expect(bullet).toHaveAttribute("data-tool-shimmer", "true");
    await expect.poll(() => bullet.evaluate((el) => getComputedStyle(el).animationName)).toBe(SHIMMER);

    await stream.send([callFrame(REQUEST_TOOL, FRAME_KIND.TOOL_CALL_COMPLETED, {
      call_id: REQUEST_CALL_ID, ms: TOOL_WALL_MS, status: "succeeded", output_head: REQUEST_OUTPUT, output_line_count: 2,
    })]);
    await expect(cell).toContainText(DONE_LABEL);
    await expect(bullet).toHaveAttribute("data-tool-shimmer", "false");
    await expect(bullet).toHaveClass(/\btext-success\b/);
    await expect.poll(() => bullet.evaluate((el) => getComputedStyle(el).animationName)).toBe("none");
    await expect(cell.getByRole("heading", {name: OUTPUT_LABEL, exact: true})).toBeVisible();
    await expect(cell).toContainText("200 OK");
    await expect(cell).toContainText("0.7s");
  });
});

// A short prompt's model reasons, answers, then reasons again before it ends.
// The reader opens and closes Thought; streaming preserves each choice.
test("test_interleaved_reasoning_folds_once", async ({ page }) => {
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream }) => {
    let seq = 0;
    const thinkFor = (count: number) => Array.from({ length: count }, () => ({
      ...chunk(seq++, "reasoning", "Deciding how to greet back. "),
      afterMs: REASONING_EVERY_MS * 2,
    }));
    await stream.send([opening(Date.now()), ...thinkFor(INTERLEAVED_CHUNKS)]);
    const thought = chat.getByRole("button", { name: /^(Thinking|Thought)/ });
    await expect(thought).toHaveAttribute("aria-expanded", "false");
    await thought.click();
    await expect(thought).toHaveAttribute("aria-expanded", "true");
    // Await the handle, not its pending result, before sending the answer.
    const sample = await thought.evaluateHandle((el, ms) => ({ states: new Promise<string[]>((resolve) => {
      const seen: string[] = [];
      const t0 = performance.now();
      const tick = () => {
        const state = el.getAttribute("aria-expanded") ?? "";
        if (seen.at(-1) !== state) seen.push(state);
        if (performance.now() - t0 < ms) requestAnimationFrame(tick);
        else resolve(seen);
      };
      // Record the reader's open choice before more content arrives.
      tick();
    }) }), INTERLEAVED_SAMPLE_MS);
    try {
      await stream.send([chunk(seq++, "answer", "Hey! ")]);
      await expect(thought).toHaveAttribute("aria-expanded", "true");
      await thought.click();
      await expect(thought).toHaveAttribute("aria-expanded", "false");
      await stream.send([...thinkFor(INTERLEAVED_CHUNKS), chunk(seq++, "answer", "How can I help?")]);
      // Only the reader's click closes it; resumed reasoning leaves it closed.
      expect(await sample.evaluate((probe) => probe.states)).toEqual(["true", "false"]);
      await stream.send([{
        afterMs: 0,
        body: frame(FRAME_KIND.EVENT_COMPLETE, { event_id: EVENT_ID, actor: ACTOR, status: "processed", final_reply: INTERLEAVED_ANSWER }),
      }]);
      await expect(chat.getByText(INTERLEAVED_ANSWER, { exact: true })).toBeVisible();
    } finally {
      await sample.dispose();
    }
  });
});

test("repeated markdown replies preserve their text when each turn completes", async ({ page }, testInfo) => {
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream, ownActor, sendOwn }) => {
    for (let turn = 0; turn < 3; turn++) {
      const eventId = `950000000000${turn}-1`;
      const intro = `Review ${turn + 1} is ready.`;
      const segments = [intro + "\n\n", "- Verify the signature.\n", "- Match the repository.\n\n",
        "```ts\n", "const verified = true;\n", "```\n\n", "Ready for the next question."];
      await sendOwn(`Please continue review ${turn + 1}.`, eventId);
      await stream.send([
        { afterMs: 0, body: frame(FRAME_KIND.EVENT_RECEIVED, { event_id: eventId, actor: ownActor, created_at: Date.now() }) },
        ...segments.map((text, seq) => ({ afterMs: ANSWER_EVERY_MS, body: frame(FRAME_KIND.CHUNK, {
          event_id: eventId, text, text_kind: "answer", stream_seq: seq, stream_start: seq === 0, stream_contiguous: true,
        }) })),
      ]);
      const paragraph = chat.getByText(intro, { exact: true });
      const row = chat.locator('[data-role="assistant"]').filter({ has: page.getByText(intro, { exact: true }) });
      await expect(row.getByText("Ready for the next question.", { exact: true })).toBeVisible();
      await expect(row.locator("pre code")).toContainText("const verified = true;");
      const probe = await paragraph.evaluateHandle(el => ({ samples: new Promise<Array<{ x: number; y: number; connected: boolean }>>(resolve => {
        const samples: Array<{ x: number; y: number; connected: boolean }> = [];
        const start = performance.now();
        const tick = () => {
          const rect = el.getBoundingClientRect();
          samples.push({ x: rect.x, y: rect.y, connected: el.isConnected });
          if (performance.now() - start < 500) requestAnimationFrame(tick);
          else resolve(samples);
        };
        tick();
      }) }));
      await stream.send([{ afterMs: 0, body: frame(FRAME_KIND.EVENT_COMPLETE, {
        event_id: eventId, actor: ownActor, status: "processed", final_reply: segments.join(""),
      }) }]);
      const samples = await probe.evaluate(async probe => probe.samples);
      await probe.dispose();
      await testInfo.attach(`conversation-turn-${turn + 1}`, { contentType: "application/json", body: JSON.stringify(samples) });
      const first = samples[0]!;
      expect(samples.every(sample => sample.connected)).toBe(true);
      expect(Math.max(...samples.map(sample => Math.abs(sample.y - first.y)))).toBeLessThanOrEqual(1);
      expect(Math.max(...samples.map(sample => Math.abs(sample.x - first.x)))).toBeLessThanOrEqual(1);
      await expect(row.getByRole("button", { name: COPY_REPLY })).toBeVisible();
    }
  });
});

test("test_streaming_reply_costs_no_long_tasks", { tag: FRAME_BUDGET_TAG }, async ({ page }, testInfo) => {
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream }) => {
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
  await withReplyPage(page, FLEET_NAME, async ({ chat, stream }) => {
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
  return callFrame(TOOL_NAME, kind, extra);
}

function callFrame(name: string, kind: string, extra: Record<string, unknown>): TimedFrame {
  return { afterMs: 0, body: frame(kind, { event_id: EVENT_ID, name, ...extra }) };
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
