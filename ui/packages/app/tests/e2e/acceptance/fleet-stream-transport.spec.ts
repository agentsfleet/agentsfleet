/** Native EventSource and the real page; fixture SSE/history responses select
 * delivery failures. Rust integration owns durable publication and recovery. */
import type { Page } from "@playwright/test";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { expect, test } from "@playwright/test";
import { FRAME_KIND } from "@/lib/api/events-types";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

const FLEET_PREFIX = "stream-transport-spec-";
const EVENT_ID = "9000000000000-1";
const MISSED_ID = "9000000000000-2";
const LIVE_MARKER = "Live completion delivered once.";
const MISSED_MARKER = "The publication missed during disconnect is recovered.";
const LIVE_SENDER = "stream-transport";
const MISSED_OUTCOME = `Lost connection to the runner — ${MISSED_MARKER}`;
const STATUS = { ACTIVE: "active", PROCESSED: "processed", FAILED: "fleet_error" } as const;
const FUTURE_RUN_OFFSET_MS = 60_000;
const SSE_CONTENT_TYPE = "text/event-stream";

test("native fleet streaming deduplicates completion and backfills after disconnect within its request budget", async ({ page }, testInfo) => {
  const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
    name: `${FLEET_PREFIX}${crypto.randomUUID()}`,
  });
  const mounted = Promise.withResolvers<void>();
  const connected = Promise.withResolvers<void>();
  const resume = Promise.withResolvers<void>();
  const closing = Promise.withResolvers<void>();
  const createdAt = Date.now() + FUTURE_RUN_OFFSET_MS;
  const href = workspaceHref(workspaceId, `fleets/${fleet.id}`);
  const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleet.id}/events/stream`;
  const historyPath = streamPath.replace(/\/stream$/, "");
  const counts = { streamRequests: 0, deliveredStreams: 0, historyReads: 0, pageRefreshes: 0 };
  const errors: string[] = [];
  const row = eventRow(createdAt);
  const complete = { ...row, fleet_status: STATUS.ACTIVE, pending_approvals: 0 };
  const overlap: EventRow = { ...row, fleet_id: fleet.id, workspace_id: workspaceId };
  const history: EventsPage = { items: [
    { ...overlap, event_id: MISSED_ID, actor: "webhook:stream-recovered",
      created_at: createdAt + 1, tokens: 77,
      status: STATUS.FAILED, failure_label: "transport_loss", failure_detail: MISSED_MARKER },
    overlap,
  ], next_cursor: null };
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route((url) => url.pathname === streamPath, async (route) => {
    counts.streamRequests += 1;
    if (counts.streamRequests === 1) {
      connected.resolve();
      await mounted.promise;
      counts.deliveredStreams += 1;
      await route.fulfill({ contentType: SSE_CONTENT_TYPE, body: openingFrames(complete) });
    } else if (counts.streamRequests === 2) {
      await resume.promise;
      counts.deliveredStreams += 1;
      await route.fulfill({ contentType: SSE_CONTENT_TYPE, body: frame(FRAME_KIND.GATE_OPENED, {
        event_id: EVENT_ID, gate_id: "reconnected-gate", pending_approvals: 2,
      }) });
    } else {
      // Finite fixture responses deliberately end. Hold the next attempt so
      // it cannot create another backfill while assertions examine recovery.
      await closing.promise;
      await route.abort();
    }
  });
  await page.route((url) => url.pathname === historyPath, async (route) => {
    counts.historyReads += 1;
    await route.fulfill({ json: history });
  });
  try {
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(href, { waitUntil: "domcontentloaded" });
    const summary = page.getByLabel("Fleet summary");
    const chat = page.getByLabel("Fleet chat");
    await expect(summary).toBeVisible();
    await connected.promise;
    recordRefreshes(page, href, counts);
    mounted.resolve();
    await expect(chat.getByText(LIVE_MARKER, { exact: true })).toHaveCount(1);
    // Pin test: these literals are the completion's user-visible figures.
    await expect(summary.getByText("12,345", { exact: true })).toBeVisible();
    await expect(summary.getByRole("link", { name: /^1 approval waiting/ })).toBeVisible();
    await afterPaint(page);
    expect(counts.historyReads).toBe(0);
    expect(counts.pageRefreshes).toBe(0);
    expect(errors).toEqual([]);

    const recoveryStarted = performance.now();
    resume.resolve();
    await expect(chat.getByText(MISSED_OUTCOME, { exact: true })).toHaveCount(1);
    await expect(summary.getByRole("link", { name: /^2 approvals waiting/ })).toBeVisible();
    // The overlap must remain one delivery, including when identical copies
    // would otherwise collapse into one grouped timeline entry.
    const overlapRow = chat.locator('[data-role="system"]').filter({
      has: page.getByText(LIVE_SENDER, { exact: true }),
    });
    await expect(overlapRow).toHaveCount(1);
    await expect(overlapRow.getByTestId("group-count")).toHaveCount(0);
    await expect(summary.getByText("77", { exact: true })).toBeVisible();
    await afterPaint(page);
    expect(counts.deliveredStreams).toBe(2);
    expect(counts.historyReads).toBe(1);
    expect(counts.pageRefreshes).toBe(0);
    expect(errors).toEqual([]);
    await testInfo.attach("fleet-stream-request-evidence", {
      contentType: "application/json",
      body: JSON.stringify({
        transport: "native EventSource with fixture SSE and history responses",
        ...counts,
        backfillUiObservationMs: performance.now() - recoveryStarted,
        latencyInterpretation: "diagnostic sample, not a backend benchmark or regression threshold",
      }),
    });
  } finally {
    mounted.resolve();
    resume.resolve();
    closing.resolve();
    await page.goto("about:blank");
    await page.unrouteAll({ behavior: "wait" });
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, FLEET_PREFIX);
  }
});

function eventRow(createdAt: number): Omit<EventRow, "fleet_id" | "workspace_id"> {
  return {
    event_id: EVENT_ID, actor: `webhook:${LIVE_SENDER}`, event_type: "webhook", status: STATUS.PROCESSED,
    created_at: createdAt, updated_at: createdAt, tokens: 12_345, wall_ms: 2_500,
    cost_nanos: 1_250_000_000, failure_label: null, failure_detail: null,
    checkpoint_id: null, resumes_event_id: null,
  };
}

function openingFrames(complete: ReturnType<typeof eventRow>): string {
  return [
    frame(FRAME_KIND.EVENT_RECEIVED, {
      event_id: EVENT_ID, actor: complete.actor, created_at: complete.created_at,
    }),
    frame(FRAME_KIND.CHUNK, { event_id: EVENT_ID, text: LIVE_MARKER }),
    frame(FRAME_KIND.EVENT_COMPLETE, complete),
    frame(FRAME_KIND.EVENT_COMPLETE, complete),
    `event: ${FRAME_KIND.EVENT_COMPLETE}\ndata: {invalid JSON\n\n`,
    frame(FRAME_KIND.EVENT_COMPLETE, { event_id: "thin-completion", status: STATUS.PROCESSED }),
    // This final count proves every preceding malformed frame was consumed.
    frame(FRAME_KIND.GATE_OPENED, {
      event_id: EVENT_ID, gate_id: "opening-gate", pending_approvals: 1,
    }),
  ].join("");
}

function frame(kind: string, payload: Record<string, unknown>): string {
  return `event: ${kind}\ndata: ${JSON.stringify({ kind, ...payload })}\n\n`;
}

function recordRefreshes(page: Page, pathname: string, counts: { pageRefreshes: number }): void {
  page.on("request", (request) => {
    if (new URL(request.url()).pathname !== pathname) return;
    const headers = request.headers();
    if (request.isNavigationRequest() || (headers.rsc === "1" && !headers["next-router-prefetch"])) {
      counts.pageRefreshes += 1;
    }
  });
}

async function afterPaint(page: Page): Promise<void> {
  await page.evaluate(() => new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  }));
}
