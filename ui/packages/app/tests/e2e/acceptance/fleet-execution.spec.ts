/**
 * fleet-execution.spec.ts — one fleet, from its first message to a finished job,
 * against the Rust daemon serving this environment.
 *
 * Wire: type a first message on New fleet → open the fleet's conversation in
 * a second tab and leave it watching → message the fleet from its own composer →
 * an online runner runs the delivery → the run reaches the provider and
 * comes back → both conversations show the persisted reply over their fleet
 * stream, and the control fleet holds no turn and no run but its own first one.
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
import type { TurnDetail } from "@/lib/api/turns";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  assertPassed,
  classifyApiFailure,
  classifyReportMissing,
  classifyStillRunning,
  classifyTerminalTurn,
  classifyIdle,
  chatTurnIds,
  failWith,
  findRunFor,
  JOURNEY_LEG,
  runIsSettled,
  readTerminalTurn,
  type JourneyLeg,
  type RunLocation,
} from "./fixtures/execution";
import { createFixtureFleet } from "./fixtures/fleet-create";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage } from "./fixtures/probe";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import { getDefaultWorkspaceId, expectFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets, purgeFixtureFleet } from "./fixtures/teardown";
import {documentIsMarked, markDocument} from "./fixtures/soft-navigation";

// The fleet that executes is made on New fleet and named by the daemon, so it
// is purged by its identity; the control fleet that must NOT see its activity
// is swept by its prefix.
const CONTROL_PREFIX = "fleet-exec-control-";

const RENDER_TIMEOUT_MS = 15_000;
// Delivery → run is the queue plus the runner heartbeat cadence, the same
// budget the failed-run walk documents.
const RUN_TIMEOUT_MS = 120_000;
// Run → terminal row includes one provider round trip. Sized for a slow
// model minute, not a stalled one: past this the run itself is what expired.
const EXECUTION_TIMEOUT_MS = 150_000;
const POLL_INTERVAL_MS = 2_000;
// The first message's own run + the two budgets above + the conversation and
// thread renders, with headroom that is deliberately NOT another retry.
const JOURNEY_TIMEOUT_MS = 420_000;

// The one-line message the fleet is asked to acknowledge. Unique per run so a
// reply, if it echoes, can be tied to this walk in a trace.
const MESSAGE_PREFIX = "acceptance-probe-";

const TURN_KIND_ACTIVITY = "activity";

// A conversation opens its own fleet stream; sidebar history opens no workspace stream.
const WORKSPACE_STREAM_SUFFIX = "/turns/stream";
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
  let executing: {workspaceId: string; fleetId: string} | null = null;

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    if (executing) await purgeFixtureFleet(FIXTURE_KEY.regular, executing.workspaceId, executing.fleetId);
    executing = null;
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, CONTROL_PREFIX);
  });

  test("a fleet made from its first message executes to a result the operator can read", {tag: "@post-deploy"}, async ({
    page,
    context,
  }, testInfo) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);

    const tag = uniqueTag();
    const controlName = `${CONTROL_PREFIX}${tag}`;
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // ── 1.1 A first message makes an active fleet ──
    await signInAs(page, FIXTURE_KEY.regular);
    const fleetId = await createFleetViaUI(page, workspaceId, probeMessage(`first-${tag}`));
    executing = {workspaceId, fleetId};
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    await observed(JOURNEY_LEG.install, () =>
      expectFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
    );
    // The control fleet: made the way a sibling would be, never messaged
    // again. Its durable history is the negative routing check.
    const control = await observed(JOURNEY_LEG.install, () =>
      createFixtureFleet(FIXTURE_KEY.regular, workspaceId, { name: controlName, message: probeMessage(`control-${tag}`) }),
    );
    await observed(JOURNEY_LEG.install, () =>
      expectFleetActive(FIXTURE_KEY.regular, workspaceId, control.fleet.id),
    );

    // A second page watches the same conversation before the delivery arrives.
    const watching = await context.newPage();
    const streams = countStreams(watching, workspaceId);
    await watching.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    await expect(watching.getByLabel(CHAT_LABEL)).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await markDocument(watching);

    // ── Trigger: the operator messages the fleet from its own composer ──
    // The first message's turn is already on the thread; the walk follows only the new one.
    const seen = await observed(JOURNEY_LEG.run, () => chatTurnIds(FIXTURE_KEY.regular, workspaceId, fleetId));
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    const composer = page.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await composer.getByPlaceholder(/message your fleet/i).fill(`${MESSAGE_PREFIX}${tag}`);
    await composer.getByRole("button", { name: /send/i }).click();

    // ── 1.2 An online runner runs the delivery ──
    const run = await pollFor<RunLocation>(
      () => observed(JOURNEY_LEG.run, () => findRunFor(fleetId, seen)),
      RUN_TIMEOUT_MS,
    );
    if (run === null) failWith(classifyIdle(await anyRunnerLive()));

    // ── 1.3 The run finishes and its result reaches the thread ──
    const terminal = await pollFor<TurnDetail>(
      () =>
        observed(JOURNEY_LEG.execute, () =>
          readTerminalTurn(FIXTURE_KEY.regular, workspaceId, fleetId, seen),
        ),
      EXECUTION_TIMEOUT_MS,
    );
    if (terminal === null) {
      // No terminal row inside the budget. A run that settled without one is
      // a report that never landed; a run still running is the provider or
      // the clock; no run at all is the runner.
      const settled = await observed(JOURNEY_LEG.execute, () => findRunFor(fleetId, seen));
      if (settled !== null && runIsSettled(settled.outcome)) {
        failWith(classifyReportMissing(settled.outcome));
      }
      // A run that exists and has NOT settled is still running. Falling
      // through to `classifyIdle` here would report "an online runner never
      // held the delivery" about a delivery this journey watched get held.
      if (settled !== null) failWith(classifyStillRunning(settled.outcome));
      failWith(classifyIdle(await anyRunnerLive()));
    }
    await testInfo.attach("terminal-turn", {
      body: JSON.stringify(
        {
          turn_id: terminal.turn_id,
          status: terminal.status,
          failure_label: terminal.failure_label,
          failure_detail: terminal.failure_detail,
          reply: terminal.response_text,
          runner: run.hostId,
        },
        null,
        2,
      ),
      contentType: "application/json",
    });
    assertPassed(classifyTerminalTurn(terminal));

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
    const observedReply = watching.getByLabel(CHAT_LABEL).locator(`[data-message-id="${terminal.turn_id}:reply"]`);
    await expect(observedReply).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(observedReply).toHaveAttribute("data-settled", "true", {timeout: RENDER_TIMEOUT_MS});
    await expect(observedReply.locator(ASSISTANT_TURN)).not.toHaveText(/^\s*$/, {timeout: RENDER_TIMEOUT_MS});
    expect(await documentIsMarked(watching)).toBe(true);
    // The control fleet holds its own first turn and nothing of this walk's.
    const controlHistory = await clientFor(FIXTURE_KEY.regular).get<{items: TurnDetail[]}>(
      `/v1/workspaces/${workspaceId}/fleets/${control.fleet.id}/messages`,
    );
    expect(controlHistory.items.filter(row => row.event_type !== TURN_KIND_ACTIVITY).map(row => row.turn_id)).toEqual([control.turnId]);
    expect(await findRunFor(control.fleet.id, new Set([control.turnId]))).toBeNull();
    expect(streams.workspace, "sidebar history opens no workspace stream").toBe(0);
    expect(streams.fleet, "the watching conversation opens one fleet stream").toBe(1);
    await watching.close();

  });
});
