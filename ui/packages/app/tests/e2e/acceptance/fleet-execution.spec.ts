/**
 * fleet-execution.spec.ts — one fleet, from a gallery card to a finished job,
 * against the Rust daemon serving this environment.
 *
 * Wire: install a fleet from the gallery through the dashboard, with a
 * SKILL.md that carries a real instruction → open the workspace wall in a
 * second tab and leave it watching → message the fleet from its own composer →
 * an online runner leases the delivery → the lease reaches the provider and
 * comes back → the thread shows the reply → the wall's tile for THIS fleet
 * carried the activity, over one workspace stream, and a control fleet's tile
 * did not.
 *
 * This is the inverse of `runner-detail.spec.ts`, which seeds an EMPTY skill
 * body so the lease fails closed before any model call. Here the body has
 * instructions, so the lease goes all the way: one real provider round trip per
 * run. Nothing model-free exists from the outside (the runner's only stub is a
 * compile-time flag), so the assertions are about the OUTCOME — a processed row
 * with a non-empty reply — and never about what the model said.
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
import { FIXTURE_KEY } from "./fixtures/constants";
import {
  anyRunnerLive,
  assertPassed,
  classifyApiFailure,
  classifyReportMissing,
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
import { installViaUI } from "./fixtures/install-ui";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import {
  executionSkillMd,
  getDefaultWorkspaceId,
  readFleetName,
  seedFleet,
  waitForFleetActive,
} from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";

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
// Install (60s inside installViaUI) + the two budgets above + the wall and
// thread renders, with headroom that is deliberately NOT another retry.
const JOURNEY_TIMEOUT_MS = 420_000;

// The one-line message the fleet is asked to acknowledge. Unique per run so a
// reply, if it echoes, can be tied to this walk in a trace.
const MESSAGE_PREFIX = "acceptance-probe-";

// Mirrors of tile copy in `fleets/components/FleetTile.tsx`. Importing the
// constants would pull a "use client" module — React and next/navigation with
// it — into the Playwright process; the wall's own unit suite pins the copy.
const TILE_WAITING_COPY = "Waiting for the next event.";
const MANAGE_FLEET_LABEL = "Manage fleet";
// Every tile card carries its kind — live, snapshot or drained — as data.
const TILE_CARD = "[data-kind]";

// The two SSE surfaces the dashboard can open. The wall must open the
// workspace one, exactly once, and no per-fleet one at all.
const WORKSPACE_STREAM_SUFFIX = "/events/stream";
const FLEET_STREAM_SEGMENT = "/fleets/";

const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";
const LEASES_TABLE_LABEL = "Runner leases";
const ASSISTANT_TURN = '[data-role="assistant"]';

function uniqueTag(): string {
  return crypto.randomUUID().slice(0, 8);
}

// The wall opens one workspace stream and no per-fleet streams. Counted from
// the request log rather than inferred from what rendered, because a tile
// that renders live from its own fleet stream looks identical to one fed by
// the workspace stream — and the difference is the whole point of the wall.
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

// A tile is the card that holds the fleet's overlay link. The link itself is
// an aria-labelled `absolute inset-0` sheet with no text of its own — the feed
// line and the counters are its siblings — so the card is what carries the
// words a person reads, and the link is only how the card is found.
function tileFor(page: Page, fleetName: string) {
  const overlay = page.getByRole("link", {
    name: new RegExp(`^${MANAGE_FLEET_LABEL}: ${fleetName} `),
  });
  return page.locator(TILE_CARD).filter({ has: overlay });
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

  test("a fleet installed from the gallery executes to a result the operator can read", async ({
    page,
    context,
  }, testInfo) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);

    const tag = uniqueTag();
    const controlName = `${CONTROL_PREFIX}${tag}`;
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // ── 1.1 A gallery install reaches active without a confirm step ──
    // installViaUI is the operator's own walk: the card's Install, the live
    // install states, Open fleet. No name field, no confirm dialog.
    await signInAs(page, FIXTURE_KEY.regular);
    const fleetId = await installViaUI(page, EXECUTING_TEMPLATE, {
      handle: FIXTURE_KEY.regular,
      workspaceId,
      skillMarkdown: executionSkillMd(EXECUTING_TEMPLATE),
    });
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleetId}`));
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
    );
    // The server may have suffixed the template's name; the tile and the
    // lease row carry whatever it chose, so read it rather than assume it.
    const name = await observed(JOURNEY_LEG.install, () =>
      readFleetName(FIXTURE_KEY.regular, workspaceId, fleetId),
    );

    // The control fleet: installed the same way a sibling would be, never
    // messaged. Its tile is the negative half of 1.4.
    const control = await observed(JOURNEY_LEG.install, () =>
      seedFleet(FIXTURE_KEY.regular, workspaceId, { name: controlName }),
    );
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, control.id),
    );

    // ── 1.4, first half: the wall is watching BEFORE anything happens ──
    // A second tab, same signed-in context. The wall has to be open while the
    // delivery runs: its tiles are fed live, and activity that happened before
    // the stream connected is history, not routing.
    const wall = await context.newPage();
    const streams = countStreams(wall, workspaceId);
    await wall.goto(workspaceHref(workspaceId, "fleets"));
    await expect(tileFor(wall, name)).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(tileFor(wall, controlName)).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(tileFor(wall, name)).toContainText(TILE_WAITING_COPY, {
      timeout: RENDER_TIMEOUT_MS,
    });

    // ── Trigger: the operator messages the fleet from its own composer ──
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    const composer = page.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await composer.getByPlaceholder(/message this fleet/i).fill(`${MESSAGE_PREFIX}${tag}`);
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

    // ── 1.4, second half: the activity reached one tile over one stream ──
    await expect(tileFor(wall, name)).not.toContainText(TILE_WAITING_COPY, {
      timeout: RENDER_TIMEOUT_MS,
    });
    await expect(tileFor(wall, controlName)).toContainText(TILE_WAITING_COPY);
    expect(streams.workspace, "the wall opens the workspace stream once").toBe(1);
    expect(streams.fleet, "the wall opens no per-fleet stream").toBe(0);
    await wall.close();

    // ── 1.2, as the operator's own view: the lease on its runner's page ──
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(`/admin/runners/${lease.runnerId}`);
    const leases = page.getByRole("table", { name: LEASES_TABLE_LABEL });
    await expect(leases).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(leases.getByRole("row").filter({ hasText: name }).first()).toBeVisible({
      timeout: RENDER_TIMEOUT_MS,
    });
  });
});
