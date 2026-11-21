/** Native EventSource and the real page; fixture SSE/history responses select
 * delivery failures. Rust integration owns durable publication and recovery. */
import type { Page } from "@playwright/test";
import type { TurnRow, TurnsPage } from "@/lib/api/turns";
import { expect, test } from "@playwright/test";
import { RUN_DETAILS_LABEL } from "@/components/domain/activity/replies/FleetReplyFigures";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { formatCompactCount } from "@/lib/turns/run-figures-format";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { SSE_CONTENT_TYPE, sseFrame as frame } from "./fixtures/sse";

const FLEET_PREFIX = "stream-transport-spec-";
const TURN_ID = "9000000000000-1";
const MISSED_ID = "9000000000000-2";
const STREAMING_ID = "9000000000000-3";
const LIVE_MARKER = "Live completion delivered once.";
const STREAMING_MARKER = "Live chunk painted before any completion.";
const STREAMING_SENDER = "stream-still-running";
const MISSED_MARKER = "The publication missed during disconnect is recovered.";
const LIVE_SENDER = "stream-transport";
// A transport loss with prose detail shows the detail alone (fleetFailureCopy.tsx).
const MISSED_OUTCOME = MISSED_MARKER;
const STATUS = { ACTIVE: "active", PROCESSED: "processed", FAILED: "fleet_error" } as const;
const FUTURE_RUN_OFFSET_MS = 60_000;
const LIVE_TOKENS = 12_345;
// FleetHeaderStatus's label for any pending approval: the one surface a
// frame's approval count reaches while no gate exists behind it.
const NEEDS_APPROVAL = "Needs approval";

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
  const streamPath = `/live/v1/workspaces/${workspaceId}/fleets/${fleet.id}/turns/stream`;
  const historyPath = streamPath.replace(/\/stream$/, "");
  const counts = { streamRequests: 0, deliveredStreams: 0, historyReads: 0, pageRefreshes: 0 };
  const errors: string[] = [];
  const row = turnRow(createdAt);
  // The daemon inlines the settled answer on the completion frame
  // (afd_fleet run/bracket.rs `publish_completion`); the browser shows it
  // without a detail read.
  const complete = { ...row, fleet_status: STATUS.ACTIVE, pending_approvals: 0, final_reply: LIVE_MARKER };
  const overlap: TurnRow = { ...row, fleet_id: fleet.id, workspace_id: workspaceId };
  const history: TurnsPage = { items: [
    { ...overlap, turn_id: MISSED_ID, actor: "webhook:stream-recovered",
      created_at: new Date(createdAt + 1).toISOString(), tokens: 77,
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
        turn_id: TURN_ID, gate_id: "reconnected-gate", pending_approvals: 2,
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
    const chat = page.getByLabel("Fleet chat");
    await expect(chat).toBeVisible();
    await connected.promise;
    recordRefreshes(page, href, counts);
    mounted.resolve();
    await expect(chat.getByText(LIVE_MARKER, { exact: true })).toHaveCount(1);
    // The completion inlines LIVE_MARKER, so it could paint on its own. This
    // run never completes: only the live chunk path can put its text on screen.
    await expect(chat.getByText(STREAMING_MARKER, { exact: true })).toHaveCount(1);
    // The completion's figures ride its frame into the reply's Run details.
    const liveReply = chat.locator('[data-role="assistant"]').filter({ hasText: LIVE_MARKER });
    await liveReply.getByRole("button", { name: RUN_DETAILS_LABEL, exact: true }).click();
    const details = page.getByRole("dialog", { name: RUN_DETAILS_LABEL });
    await expect(details.getByText(formatCompactCount(LIVE_TOKENS), { exact: true })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(details).toBeHidden();
    // Only the final opening frame carries a pending approval, so the label
    // proves every malformed frame before it was consumed.
    await expect(page.getByText(NEEDS_APPROVAL, { exact: true })).toBeVisible();
    await afterPaint(page);
    expect(counts.historyReads).toBe(0);
    expect(counts.pageRefreshes).toBe(0);
    expect(errors).toEqual([]);

    const recoveryStarted = performance.now();
    resume.resolve();
    await expect(chat.locator("[data-failed-outcome]").getByText(MISSED_OUTCOME, { exact: true })).toHaveCount(1);
    // The newest row is now a failure, which would read "Needs attention" had
    // the backfill dropped the gate facts. The label is binary, so it cannot
    // tell the reconnected gate's 2 from 1: FleetApprovalSummary settles to
    // nothing when the API returns no gate, and these gates exist only in frames.
    await expect(page.getByText(NEEDS_APPROVAL, { exact: true })).toBeVisible();
    // The overlap must remain one delivery, including when identical copies
    // would otherwise collapse into one grouped timeline entry.
    const overlapRow = chat.locator('[data-role="system"]').filter({
      has: page.getByText(LIVE_SENDER, { exact: true }),
    });
    await expect(overlapRow).toHaveCount(1);
    await expect(overlapRow.getByTestId("group-count")).toHaveCount(0);
    // The missed row's 77 tokens have no surface: an integration turn with no
    // reply earns no reply row (useFleetThreadEntries `hasReplyRow`), and its
    // tick carries no run figures. The single outcome above is its proof.
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

function turnRow(createdAt: number): Omit<TurnRow, "fleet_id" | "workspace_id"> {
  return {
    turn_id: TURN_ID, actor: `webhook:${LIVE_SENDER}`, event_type: "webhook", status: STATUS.PROCESSED,
    created_at: new Date(createdAt).toISOString(), updated_at: new Date(createdAt).toISOString(), tokens: LIVE_TOKENS, wall_ms: 2_500,
    cost_nanos: "1250000000", failure_label: null, failure_detail: null,
    resumes_turn_id: null,
  };
}

function openingFrames(complete: ReturnType<typeof turnRow>): string {
  return [
    frame(FRAME_KIND.TURN_RECEIVED, {
      turn_id: TURN_ID, actor: complete.actor, created_at: complete.created_at,
    }),
    // The runner's chunk shape since typed streaming: an untyped or
    // unsequenced chunk is treated as a gap and never painted.
    frame(FRAME_KIND.CHUNK, {
      turn_id: TURN_ID, text: LIVE_MARKER, text_kind: "answer",
      stream_seq: 0, stream_start: true, stream_contiguous: true,
    }),
    frame(FRAME_KIND.TURN_COMPLETE, complete),
    frame(FRAME_KIND.TURN_COMPLETE, complete),
    `event: ${FRAME_KIND.TURN_COMPLETE}\ndata: {invalid JSON\n\n`,
    frame(FRAME_KIND.TURN_COMPLETE, { turn_id: "thin-completion", status: STATUS.PROCESSED }),
    // Older than the completed run, so the latest-run figures stay that run's
    // instead of this one's empty ones.
    frame(FRAME_KIND.TURN_RECEIVED, {
      turn_id: STREAMING_ID, actor: `webhook:${STREAMING_SENDER}`, created_at: new Date(Date.parse(complete.created_at) - 1).toISOString(),
    }),
    frame(FRAME_KIND.CHUNK, {
      turn_id: STREAMING_ID, text: STREAMING_MARKER, text_kind: "answer",
      stream_seq: 0, stream_start: true, stream_contiguous: true,
    }),
    // This final count proves every preceding malformed frame was consumed.
    frame(FRAME_KIND.GATE_OPENED, {
      turn_id: TURN_ID, gate_id: "opening-gate", pending_approvals: 1,
    }),
  ].join("");
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
