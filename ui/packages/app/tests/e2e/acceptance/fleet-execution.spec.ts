import { executionInstructions } from "./fixtures/definition";
/**
 * fleet-execution.spec.ts — one fleet, from a reviewed chat setup to a finished job,
 * against the Rust daemon serving this environment.
 *
 * Wire: save a fleet from the private chat's inline setup, with instructions
 * that carry a real request → open the same conversation in a
 * second tab and leave it watching → message the fleet from its own composer →
 * an online runner leases the delivery → the lease reaches the provider and
 * comes back → both conversations show the persisted reply over their fleet
 * stream, and the untouched control fleet has no chat delivery or lease.
 *
 * What a failure prints is part of the contract. `fixtures/execution.ts`
 * classifies every terminal row and every API refusal, and the walk throws its
 * verdict as one line: `[environment] execute: …` for a provider or runner
 * condition, `[product] execute: …` for a defect. Every read that can meet the
 * environment goes through `observed`, so an unclassified assertion failure
 * left over is, by construction, the dashboard failing to show a fact the
 * daemon holds — a product failure at the observe leg (RULE ECL).
 */
import { expect, test, type Page } from "@playwright/test";
import type { EventDetail } from "@/lib/api/events";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  assertPassed,
  classifyApiFailure,
  classifyReportMissing,
  classifyStillRunning,
  classifyTerminalEvent,
  classifyUnleased,
  failWith,
  findLeaseFor,
  JOURNEY_LEG,
  leaseIsSettled,
  readTerminalChatTurn,
  type JourneyLeg,
  type LeaseLocation,
} from "./fixtures/execution";
import { saveFleetViaUI } from "./fixtures/install-ui";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import {documentIsMarked, markDocument} from "./fixtures/soft-navigation";

// One stable template for the fleet that executes — the server suffixes a
// repeat — and one prefix for the control fleet that must NOT see its
// activity. Both carry a live cron trigger, so the afterEach sweep is what
// stops them waking runners after the run; one sweep prefix reaches both.
const EXECUTING_TEMPLATE = "fleet-exec";
const CONTROL_PREFIX = "fleet-exec-control-";
const SWEEP_PREFIX = EXECUTING_TEMPLATE;

const RENDER_TIMEOUT_MS = 15_000;
// Delivery → lease is the queue plus the runner heartbeat cadence, the same
// budget the failed-lease walk documents.
const LEASE_TIMEOUT_MS = 120_000;
// Lease → terminal row includes one provider round trip. Sized for a slow
// model minute, not a stalled one: past this the lease itself is what expired.
const EXECUTION_TIMEOUT_MS = 150_000;
const POLL_INTERVAL_MS = 2_000;
// Install (60s inside saveFleetViaUI) + the two budgets above + the conversation and
// thread renders, with headroom that is deliberately NOT another retry.
const JOURNEY_TIMEOUT_MS = 420_000;

// The one-line message the fleet is asked to acknowledge. Unique per run so a
// reply, if it echoes, can be tied to this walk in a trace.
const MESSAGE_PREFIX = "acceptance-probe-";

const ACTIVITY_EVENT = "activity";

// A conversation opens its own fleet stream; sidebar history opens no workspace stream.
const WORKSPACE_STREAM_SUFFIX = "/events/stream";
const FLEET_STREAM_SEGMENT = "/fleets/";

const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";
const ASSISTANT_TURN = '[data-role="assistant"]';
const FIRST_VISIBLE_MEASURE = "agentsfleet.chat.submit_to_first_visible";

function uniqueTag(): string {
  return crypto.randomUUID().slice(0, 8);
}

// Count actual stream requests rather than inferring transport from rendered text.
interface StreamCounts {
  workspace: number;
  fleet: number;
}

function countStreams(page: Page, workspaceId: string): StreamCounts {
  const counts: StreamCounts = { workspace: 0, fleet: 0 };
  const workspaceStream = `/live/v1/workspaces/${workspaceId}${WORKSPACE_STREAM_SUFFIX}`;
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (!url.pathname.endsWith(WORKSPACE_STREAM_SUFFIX)) return;
    if (url.pathname === workspaceStream) counts.workspace += 1;
    else if (url.pathname.includes(FLEET_STREAM_SEGMENT)) counts.fleet += 1;
  });
  return counts;
}

// Every API read inside the walk goes through here, so a daemon that stops
// answering mid-journey is named as such — with the leg it broke on — rather
// than surfacing as a bare fetch failure.
async function observed<T>(leg: JourneyLeg, read: () => Promise<T>): Promise<T> {
  try {
    return await read();
  } catch (error) {
    return failWith(classifyApiFailure(error, leg));
  }
}

// A bounded poll that answers `null` when the budget runs out, so the caller
// classifies the silence itself rather than reading a timeout as a verdict.
async function pollFor<T>(read: () => Promise<T | null>, timeoutMs: number): Promise<T | null> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await read();
    if (value !== null) return value;
    if (Date.now() > deadline) return null;
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
}

test.describe("fleet execution", () => {
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, SWEEP_PREFIX);
  });

  test("a fleet installed from the gallery executes to a result the operator can read", {tag: "@post-deploy"}, async ({
    page,
    context,
  }, testInfo) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);

    const tag = uniqueTag();
    const controlName = `${CONTROL_PREFIX}${tag}`;
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // ── 1.1 A reviewed save reaches active ──
    // Edit instructions, save without leaving authoring, then open the fleet.
    await signInAs(page, FIXTURE_KEY.regular);
    const fleetId = await saveFleetViaUI(page, EXECUTING_TEMPLATE, {
      handle: FIXTURE_KEY.regular,
      workspaceId,
      instructions: executionInstructions(),
    });
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
    );
    // The control fleet: installed the same way a sibling would be, never
    // messaged. Its durable history is the negative routing check.
    const control = await observed(JOURNEY_LEG.install, () =>
      seedFleet(FIXTURE_KEY.regular, workspaceId, { name: controlName }),
    );
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, control.id),
    );

    // A second page watches the same conversation before the delivery arrives.
    const watching = await context.newPage();
    const streams = countStreams(watching, workspaceId);
    await watching.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await expect(watching.getByLabel(CHAT_LABEL)).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await markDocument(watching);

    // ── Trigger: the operator messages the fleet from its own composer ──
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    const composer = page.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await composer.getByPlaceholder(/message your fleet/i).fill(`${MESSAGE_PREFIX}${tag}`);
    await composer.getByRole("button", { name: /send/i }).click();

    // ── 1.2 An online runner leases the delivery ──
    const lease = await pollFor<LeaseLocation>(
      () => observed(JOURNEY_LEG.lease, () => findLeaseFor(fleetId)),
      LEASE_TIMEOUT_MS,
    );
    if (lease === null) failWith(classifyUnleased(await anyRunnerLive()));

    // ── 1.3 The lease finishes and its result reaches the thread ──
    const terminal = await pollFor<EventDetail>(
      () =>
        observed(JOURNEY_LEG.execute, () =>
          readTerminalChatTurn(FIXTURE_KEY.regular, workspaceId, fleetId),
        ),
      EXECUTION_TIMEOUT_MS,
    );
    if (terminal === null) {
      // No terminal row inside the budget. A lease that settled without one is
      // a report that never landed; a lease still running is the provider or
      // the clock; no lease at all is the runner.
      const settled = await observed(JOURNEY_LEG.execute, () => findLeaseFor(fleetId));
      if (settled !== null && leaseIsSettled(settled.outcome)) {
        failWith(classifyReportMissing(settled.outcome));
      }
      // A lease that exists and has NOT settled is still running. Falling
      // through to `classifyUnleased` here would report "an online runner never
      // leased the delivery" about a delivery this journey watched get leased.
      if (settled !== null) failWith(classifyStillRunning(settled.outcome));
      failWith(classifyUnleased(await anyRunnerLive()));
    }
    await testInfo.attach("terminal-turn", {
      body: JSON.stringify(
        {
          event_id: terminal.event_id,
          status: terminal.status,
          failure_label: terminal.failure_label,
          failure_detail: terminal.failure_detail,
          reply: terminal.response_text,
          runner: lease.hostId,
        },
        null,
        2,
      ),
      contentType: "application/json",
    });
    assertPassed(classifyTerminalEvent(terminal));

    // The same reply, as the operator sees it: an assistant turn on the thread
    // with text in it. The row is what the daemon persisted; this is the
    // surface the person reads, and the two are asserted apart on purpose.
    const reply = page.getByLabel(CHAT_LABEL).locator(ASSISTANT_TURN).last();
    await expect(reply).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(reply).not.toHaveText(/^\s*$/);
    const submitToVisibleMs = await pollFor(
      () => page.evaluate((name) => performance.getEntriesByName(name).at(-1)?.duration ?? null, FIRST_VISIBLE_MEASURE),
      RENDER_TIMEOUT_MS,
    );
    expect(submitToVisibleMs, "the browser records submit-to-first-visible latency").not.toBeNull();
    await testInfo.attach("submit-to-first-visible-ms", {
      body: JSON.stringify({ duration_ms: submitToVisibleMs }),
      contentType: "application/json",
    });

    // The persisted turn also appears in the untouched watching conversation.
    const observedReply = watching.getByLabel(CHAT_LABEL).locator(`[data-message-id="${terminal.event_id}:reply"]`);
    await expect(observedReply).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(observedReply).toHaveAttribute("data-settled", "true", {timeout: RENDER_TIMEOUT_MS});
    await expect(observedReply.locator(ASSISTANT_TURN)).not.toHaveText(/^\s*$/, {timeout: RENDER_TIMEOUT_MS});
    expect(await documentIsMarked(watching)).toBe(true);
    // A setup receipt may appear on an untouched fleet; execution must not.
    const controlHistory = await clientFor(FIXTURE_KEY.regular).get<{items: EventDetail[]}>(
      `/v1/workspaces/${workspaceId}/fleets/${control.id}/messages`,
    );
    expect(controlHistory.items.every(row => row.event_type === ACTIVITY_EVENT)).toBe(true);
    expect(await findLeaseFor(control.id)).toBeNull();
    expect(streams.workspace, "sidebar history opens no workspace stream").toBe(0);
    expect(streams.fleet, "the watching conversation opens one fleet stream").toBe(1);
    await watching.close();

  });
});
