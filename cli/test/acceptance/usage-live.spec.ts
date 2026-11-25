/**
 * usage-live — what a run cost, read the way a terminal user reads it.
 *
 * A morning-rooster, the prime set's plainest fleet, answers one message. The
 * tenant's balance falls, `usage show` prints the run's charge as a row, and
 * its cursor pages back without repeating one.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { entry, primeFleet as primeFixture } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";
import { STEER_TIMEOUT_MS, awaitTurn, processed, steer } from "./fixtures/prime-turns.ts";
import { chargedNanos, readUsagePage, readUsageTextTotal } from "./fixtures/usage-ops.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";
import { instantMs } from "../../src/lib/instant.ts";

const fixture = primeFixture("morning-rooster");
const followUp = entry(fixture.follow_ups, 0, "follow-up");
// This file's own workspace: other files create a morning-rooster too, and a
// workspace holds one fleet of a name.
const WORKSPACE = "fixture-cli-usage";

const JSON_FLAG = "--json" as const;
const BILLING_ARGS = ["usage", "show", JSON_FLAG] as const;
const BALANCE_FIELD = "balance_nanos" as const;
// The create and its first turn's run.
const SETUP_TIMEOUT_MS = 360_000;
// The usage ledger settles after the terminal row; the balance follows it.
const BALANCE_SETTLE_MS = 90_000;
const BALANCE_POLL_MS = 3_000;
// A dollar figure as `formatDollars` (src/constants/billing.ts) renders one.
const DOLLARS = /^-?\$[\d,]+\.\d{2,}$/u;
// Decodes to no boundary the charges route can read, so no page issued it
// (`afd_billing::tenant::cursor::parse`).
const FOREIGN_CURSOR = "not-a-cursor" as const;
// The charges route refuses a cursor it cannot parse as INVALID_REQUEST
// (afd_billing/src/error/mod.rs).
const CODE_INVALID_REQUEST = "UZ-REQ-001" as const;

if (!isLive) {
  describe("usage-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("usage-live — a run depletes the balance, and `usage show` lists its charge", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    const runWithEnv = (args: ReadonlyArray<string>) => session.run(args, STEER_TIMEOUT_MS);
    let fleetId = "";
    let balanceBeforeNanos = 0n;
    let steeredTurnId = "";

    async function readBalanceNanos(): Promise<bigint> {
      const result = await runWithEnv([...BILLING_ARGS]);
      assert.equal(result.code, 0, `billing show exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
      const balance = parsed[BALANCE_FIELD];
      assert.equal(typeof balance, "string", `billing show carried no ${BALANCE_FIELD}`);
      assert.match(String(balance), /^(0|[1-9][0-9]*)$/);
      return BigInt(String(balance));
    }

    beforeAll(async () => {
      balanceBeforeNanos = await readBalanceNanos();
      // The first turn is waited out, so the message below starts a run of its
      // own and is billed under its own turn.
      const created = await primeFleet(session, fixture);
      fleetId = created.fleet_id;
      await awaitTurn(session, fleetId, processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
    }, SETUP_TIMEOUT_MS);

    it("the fleet answers a steer, and the tenant balance falls", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      steeredTurnId = await steer(session, fleetId, followUp);

      // Nanos, never a rendered figure: a run costing a fraction of a cent
      // moves the balance and no two-decimal display.
      const deadline = Date.now() + BALANCE_SETTLE_MS;
      let balanceAfterNanos = await readBalanceNanos();
      while (balanceAfterNanos >= balanceBeforeNanos && Date.now() < deadline) {
        await Bun.sleep(BALANCE_POLL_MS);
        balanceAfterNanos = await readBalanceNanos();
      }
      assert.ok(balanceAfterNanos < balanceBeforeNanos,
        `the run did not deplete credit: ${balanceBeforeNanos} → ${balanceAfterNanos} nanos`);
    }, STEER_TIMEOUT_MS + BALANCE_SETTLE_MS);

    it("`usage show` prints a dollar total for the run and JSON shows a positive charge for this fleet", async () => {
      assert.ok(steeredTurnId, "the steer did not run");
      const printedTotal = await readUsageTextTotal(runWithEnv, steeredTurnId);
      assert.match(printedTotal, DOLLARS, `the run's total is not a dollar figure: ${printedTotal}`);

      const charged = await chargedNanos(runWithEnv, fleetId, steeredTurnId);
      assert.ok(charged > 0n, `usage show charged the run nothing: ${charged} nanos`);
    }, STEER_TIMEOUT_MS);

    it("`usage show --cursor` continues to an older charge without repeating the first page's", async () => {
      assert.ok(steeredTurnId, "the steer did not run");
      const first = await readUsagePage(runWithEnv, 1);
      assert.equal(first.events.length, 1, `a one-row page grouped into ${first.events.length} entries`);
      assert.ok(first.next_cursor, "a full page must offer the cursor that continues it");
      // A run writes a model row per request and the daemon adds a runtime
      // row (af_wire/src/usage/mod.rs), so the run alone fills a second page.
      const second = await readUsagePage(runWithEnv, 1, first.next_cursor);
      assert.equal(second.events.length, 1, "the page after a full one came back empty");
      // The daemon renders a cursor from its page's last row, so an equal
      // cursor means page two handed back page one's row.
      assert.notEqual(second.next_cursor, first.next_cursor, "page two repeated page one's row");
      const newer = instantMs(first.events[0]?.recorded_at);
      const older = instantMs(second.events[0]?.recorded_at);
      assert.ok(newer !== null && older !== null && older <= newer,
        `the walk went forward in time: ${JSON.stringify([first, second])}`);
    });

    it("`usage show --cursor` with a token no page issued is refused by the daemon", async () => {
      const refused = await runWithEnv(["usage", "show", "--cursor", FOREIGN_CURSOR, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a foreign cursor was accepted: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_INVALID_REQUEST);
    });
  });
}
