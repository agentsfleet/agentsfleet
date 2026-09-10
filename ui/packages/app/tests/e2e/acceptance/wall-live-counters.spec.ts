/**
 * wall-live-counters.spec.ts — one operator's chat moves another operator's
 * wall, and moves the NUMBERS.
 *
 * Two browser sessions of the same person: one leaves the Fleets wall open and
 * never touches it again, the other opens a fleet and sends it a message. The
 * claim is that the untouched wall's tile for that agent advances its own
 * counters — not that its placeholder copy clears.
 *
 * # Why the distinction is the whole test
 *
 * The tile's feed line and its footer figures come from different places. The
 * feed line reads the last streamed row, so it clears the moment any frame
 * arrives; the footer reads the counter snapshot those frames CARRY, assigned
 * by the stream store (`lib/streaming/workspace-store.ts`), and a tile that
 * renders the raw `fleet.*` server render instead looks identical until you
 * read the digits.
 * That is the defect this walk exists to catch, so "the copy changed" is not
 * accepted as evidence and the event count is compared as a number.
 *
 * # Why two contexts and not two tabs
 *
 * `context.newPage()` shares a cookie jar and every in-memory store with the
 * first tab, so two tabs can agree about a number neither page's socket ever
 * received. Two contexts share nothing but the identity, which is what makes
 * this a claim about the workspace stream rather than about React state.
 *
 * # What the watching page is never allowed to do
 *
 * Reload, navigate, or re-render from the server. The walk proves it did not:
 * the watching page's `performance` navigation entry count must still be one
 * when the assertions run. A `goto` here would turn a live-update claim into a
 * server-render claim and pass under the very bug it is written for.
 *
 * # The spend cell rounds, and the walk says so
 *
 * `formatTileSpend` renders two decimals. A run charged less than half a cent
 * cannot move that cell however correct the plumbing is, so the spend assertion
 * is made conditional on the SERVER's own before/after figures crossing a cent
 * boundary — measured, attached as evidence, never assumed. The event counter
 * carries the unconditional claim.
 */
import { expect, test, type Page } from "@playwright/test";
import { signedInContext } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { deriveFleetIdentity } from "@/lib/fleets/identity";
import {
  assertPassed,
  classifyTerminalEvent,
  JOURNEY_LEG,
} from "./fixtures/execution";
import { awaitTerminalTurn, messageFleet } from "./fixtures/chat-walk";
import { observed, uniqueTag } from "./fixtures/observation";
import { installViaUI } from "./fixtures/install-ui";
import { workspaceHref } from "./fixtures/nav";
import {
  executionSkillMd,
  getDefaultWorkspaceId,
  readFleetCounters,
  seedFleet,
  waitForFleetActive,
} from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import {
  readTileCounters,
  renderTileSpend,
  spendMoved,
  tileForAgent,
  TILE_FIGURE_ABSENT,
} from "./fixtures/wall-tile";

// One stable template name — the server suffixes a repeat rather than refusing
// it, so a per-run unique name would mint a gallery row per run that nothing
// deletes. The control fleet's prefix extends it, so one sweep reaches both.
const TEMPLATE = "wall-live";
const CONTROL_PREFIX = "wall-live-control-";
const SWEEP_PREFIX = TEMPLATE;

const RENDER_TIMEOUT_MS = 15_000;
// The frame has to cross the daemon, Redis and one SSE hop after the terminal
// row lands. Generous, because a slow relay here is not the claim under test —
// the claim is that the figure moves at all.
const COUNTER_TIMEOUT_MS = 60_000;
// Install plus one provider round trip, with headroom that is not another try.
const JOURNEY_TIMEOUT_MS = 420_000;

const MESSAGE_PREFIX = "wall-live-probe-";

/** How many document navigations a page has made. One means it was loaded once
 * and never reloaded — the property the whole walk rests on. */
async function navigationCount(page: Page): Promise<number> {
  return page.evaluate(() => performance.getEntriesByType("navigation").length);
}

test.describe("wall live counters", () => {
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, SWEEP_PREFIX);
  });

  test("a chat in one session advances the other session's tile counters with no reload", async ({
    browser,
  }, testInfo) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);

    // ── The acting session installs the fleet it will message ──
    const acting = await signedInContext(browser, FIXTURE_KEY.regular);
    const page = acting.page;
    const fleetId = await installViaUI(page, TEMPLATE, {
      handle: FIXTURE_KEY.regular,
      workspaceId,
      skillMarkdown: executionSkillMd(TEMPLATE),
    });
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId),
    );
    // The control: seeded the same way a sibling would be, never messaged. Its
    // tile is what makes "the counters advanced" a routing claim rather than a
    // re-render claim.
    const controlName = `${CONTROL_PREFIX}${uniqueTag()}`;
    const control = await observed(JOURNEY_LEG.install, () =>
      seedFleet(FIXTURE_KEY.regular, workspaceId, { name: controlName }),
    );
    await observed(JOURNEY_LEG.install, () =>
      waitForFleetActive(FIXTURE_KEY.regular, workspaceId, control.id),
    );

    const callsign = deriveFleetIdentity(fleetId).callsign;
    const controlCallsign = deriveFleetIdentity(control.id).callsign;
    const spendBefore = (
      await observed(JOURNEY_LEG.observe, () =>
        readFleetCounters(FIXTURE_KEY.regular, workspaceId, fleetId),
      )
    ).budget_used_nanos;

    // ── The watching session opens the wall, and then does nothing ──
    const watching = await signedInContext(browser, FIXTURE_KEY.regular);
    await watching.page.goto(workspaceHref(workspaceId, "fleets"));
    const tile = tileForAgent(watching.page, callsign);
    const controlTile = tileForAgent(watching.page, controlCallsign);
    await expect(tile).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(controlTile).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const baseline = await readTileCounters(tile);
    const controlBaseline = await readTileCounters(controlTile);
    expect(baseline.events, "the tile printed no event count to advance").not.toBeNull();

    // ── The acting session messages the fleet, and the work finishes ──
    await messageFleet(page, workspaceId, fleetId, `${MESSAGE_PREFIX}${uniqueTag()}`);
    assertPassed(classifyTerminalEvent(await awaitTerminalTurn(workspaceId, fleetId)));
    const spendAfter = (
      await observed(JOURNEY_LEG.observe, () =>
        readFleetCounters(FIXTURE_KEY.regular, workspaceId, fleetId),
      )
    ).budget_used_nanos;
    await testInfo.attach("server-spend", {
      body: JSON.stringify(
        {
          fleet_id: fleetId,
          callsign,
          budget_used_nanos_before: spendBefore,
          budget_used_nanos_after: spendAfter,
          tile_spend_before: renderTileSpend(spendBefore),
          tile_spend_after: renderTileSpend(spendAfter),
          cent_boundary_crossed: spendMoved(spendBefore, spendAfter),
        },
        null,
        2,
      ),
      contentType: "application/json",
    });

    // ── The watching session's tile advanced, and it never reloaded ──
    // The event count, as a number. The copy clearing is not accepted as
    // evidence: it clears on any frame, including one whose figures the tile
    // then declines to read.
    await expect
      .poll(async () => (await readTileCounters(tile)).events, { timeout: COUNTER_TIMEOUT_MS })
      .toBeGreaterThan(baseline.events ?? 0);

    const advanced = await readTileCounters(tile);
    expect(advanced.spend, "the tile's spend cell went blank").not.toBe(TILE_FIGURE_ABSENT);
    if (spendMoved(spendBefore, spendAfter)) {
      // The charge is large enough for a two-decimal figure to show it, so a
      // tile still printing the pre-run cents is reading the stale snapshot.
      expect(advanced.spend, "the spend the run cost never reached the tile").not.toBe(
        baseline.spend,
      );
    }

    // Nothing else moved, and nothing was re-fetched to make it move.
    const controlAfter = await readTileCounters(controlTile);
    expect(controlAfter.events, "an unmessaged fleet's tile advanced").toBe(
      controlBaseline.events,
    );
    expect(
      await navigationCount(watching.page),
      "the watching page navigated; a live-update claim cannot ride a re-render",
    ).toBe(1);

    await watching.context.close();
    await acting.context.close();
  });
});
