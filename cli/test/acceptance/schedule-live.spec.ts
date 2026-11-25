/**
 * schedule-live — "run it every morning", set up and taken down from the
 * terminal.
 *
 * One hosted schedule on a live fleet, through every verb a person uses on it:
 * create, list, update, sync and delete, then the list again. The refusals
 * ride along: an expression the daemon will not register leaves no row, an
 * update with nothing in it never leaves the machine, and a deleted schedule
 * answers as unknown.
 *
 * The fleet is a morning-rooster, the prime set's plainest fleet: it adds no
 * schedule of its own, so every row here is one this file wrote. A schedule a
 * fleet adds for itself is prime-night-owl-live.spec.ts's to prove.
 *
 * # Where the scheduler comes from
 *
 * The daemon owns no timer. It stores a schedule and registers it with an
 * external scheduler (`rustd/crates/daemon/afd_cron/src/lib.rs`), reading that
 * scheduler's credentials from the `qstash` bag in the platform workspace's
 * vault (`afd_cron/src/credentials/mod.rs`). The local stack runs the Upstash
 * QStash emulator as the compose `qstash` service, and
 * `deploy/local/bootstrap.ts` (`configureScheduler`) writes the bag from the
 * emulator's own signing keys; the development deployment's bag names Upstash.
 * Sync and delete therefore reach a real scheduler in either lane, and a red
 * `synced` or a row surviving its delete is a registration that failed.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { primeFleet as primeFixture } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";
import { SCHEDULE_STATUS } from "../../src/commands/schedule/read.ts";
import { REJECTION_CODE } from "../../src/constants/rejection.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";
import { instantMs } from "../../src/lib/instant.ts";

const fixture = primeFixture("morning-rooster");
// This file's own workspace: other files create a morning-rooster too, and a
// workspace holds one fleet of a name.
const WORKSPACE = "fixture-cli-schedules";

// A schedule the daemon registered with the external scheduler.
const SCHEDULE_SYNCED = "synced" as const;

const JSON_FLAG = "--json" as const;
const SCHEDULE = ["fleet", "schedule"] as const;
const SETUP_TIMEOUT_MS = 180_000;
const MORNING_CRON = "0 7 * * *" as const;
const WEEKDAY_CRON = "30 8 * * 1-5" as const;
const MORNING_TIMEZONE = "Europe/London" as const;
const MORNING_MESSAGE = "Summarise the overnight alerts and name the three that need a person" as const;
const WEEKDAY_MESSAGE = "Summarise the overnight alerts and name each one's owner" as const;
// Prose a person might type, which no cron grammar reads.
const MALFORMED_CRON = "every morning" as const;
// `afd_api_wire::schedule::RunState::Scheduled`: registered, with a next run.
const RUN_STATE_SCHEDULED = "scheduled" as const;
// `afd_http::handler::schedule::checked` answers a field the daemon will not
// register with INVALID_REQUEST; a schedule the fleet does not hold answers
// SCHEDULE_NOT_FOUND (af_core/src/error_code/request.rs).
const CODE_INVALID_REQUEST = "UZ-REQ-001" as const;
const CODE_SCHEDULE_NOT_FOUND = "UZ-SCHED-002" as const;

const ViewSchema = Schema.Struct({
  schedule_id: Schema.String,
  fleet_id: Schema.String,
  cron: Schema.String,
  timezone: Schema.String,
  message: Schema.String,
  status: Schema.String,
  sync: Schema.String,
  run_state: Schema.String,
  next_run_at: Schema.NullOr(Schema.String),
});
const PageSchema = Schema.Struct({ schedules: Schema.Array(ViewSchema) });
const DeletedSchema = Schema.Struct({ deleted: Schema.Literal(true), schedule_id: Schema.String });

if (!isLive) {
  describe("schedule-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("schedule-live — create, list, update, sync and delete a morning schedule", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    const run = session.run;
    let fleetId = "";
    let scheduleId = "";
    let scheduleDeleted = false;

    async function view(args: ReadonlyArray<string>): Promise<typeof ViewSchema.Type> {
      const result = await run([...SCHEDULE, ...args, JSON_FLAG]);
      assert.equal(result.code, 0, `schedule ${args[0]} failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(ViewSchema)(JSON.parse(result.stdout.trim()));
    }

    async function listed(): Promise<ReadonlyArray<typeof ViewSchema.Type>> {
      const result = await run([...SCHEDULE, "list", fleetId, JSON_FLAG]);
      assert.equal(result.code, 0, `schedule list failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(PageSchema)(JSON.parse(result.stdout.trim())).schedules;
    }

    beforeAll(async () => {
      fleetId = (await primeFleet(session, fixture)).fleet_id;
      // Unregister first: a fleet deleted under a live registration leaves
      // the scheduler calling a callback nothing resolves. Teardown runs its
      // steps newest first, so this one runs before the fleet's delete.
      session.onClose(async () => {
        if (scheduleId && !scheduleDeleted) await session.json([...SCHEDULE, "delete", fleetId, scheduleId]);
      });
    }, SETUP_TIMEOUT_MS);

    it("`schedule create` saves a morning schedule for the fleet", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const created = await view(["create", fleetId, "--cron", MORNING_CRON,
        "--message", MORNING_MESSAGE, "--timezone", MORNING_TIMEZONE]);
      assert.equal(created.fleet_id, fleetId);
      assert.equal(created.cron, MORNING_CRON);
      assert.equal(created.timezone, MORNING_TIMEZONE);
      assert.equal(created.message, MORNING_MESSAGE);
      assert.equal(created.status, SCHEDULE_STATUS.active);
      scheduleId = created.schedule_id;
    });

    it("`schedule list` shows the new schedule", async () => {
      assert.ok(scheduleId, "no schedule was created");
      const row = (await listed()).find((schedule) => schedule.schedule_id === scheduleId);
      assert.ok(row, `schedule list does not show ${scheduleId}`);
      assert.equal(row.cron, MORNING_CRON);
      assert.equal(row.message, MORNING_MESSAGE);
    });

    it("`schedule update` moves the time and rewords the message in place", async () => {
      assert.ok(scheduleId, "no schedule was created");
      const updated = await view(["update", fleetId, scheduleId, "--cron", WEEKDAY_CRON,
        "--message", WEEKDAY_MESSAGE]);
      assert.equal(updated.schedule_id, scheduleId, "an update must change the schedule, never replace it");
      assert.equal(updated.cron, WEEKDAY_CRON);
      assert.equal(updated.message, WEEKDAY_MESSAGE);
      assert.equal(updated.timezone, MORNING_TIMEZONE, "an update that names no zone keeps the one it had");

      const shown = await view(["show", fleetId, scheduleId]);
      assert.equal(shown.cron, WEEKDAY_CRON);
      assert.equal(shown.message, WEEKDAY_MESSAGE);
    });

    it("`schedule sync` registers it with the scheduler, which names the next run", async () => {
      assert.ok(scheduleId, "no schedule was created");
      const synced = await view(["sync", fleetId, scheduleId]);
      assert.equal(synced.sync, SCHEDULE_SYNCED,
        `the scheduler did not take the registration: ${JSON.stringify(synced)}`);
      assert.equal(synced.run_state, RUN_STATE_SCHEDULED);
      assert.notEqual(instantMs(synced.next_run_at), null,
        `a registered schedule must name its next run: ${JSON.stringify(synced)}`);
      assert.equal(synced.cron, WEEKDAY_CRON, "sync must register the saved expression, not the first one");
    });

    it("an expression the daemon cannot read is refused and leaves no row", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const before = (await listed()).length;
      const refused = await run([...SCHEDULE, "create", fleetId, "--cron", MALFORMED_CRON,
        "--message", MORNING_MESSAGE, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `a malformed cron was accepted: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_INVALID_REQUEST);
      const after = await listed();
      assert.equal(after.length, before, "a refused create must write no schedule");
      assert.ok(after.every((schedule) => schedule.cron !== MALFORMED_CRON));
    });

    it("`schedule update` with nothing to change is refused before any request", async () => {
      assert.ok(scheduleId, "no schedule was created");
      const refused = await run([...SCHEDULE, "update", fleetId, scheduleId, JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ValidationError, `an empty update ran: ${refused.stdout}`);
      assert.equal(refusalCode(refused), REJECTION_CODE.invalidArgument);
      assert.equal((await view(["show", fleetId, scheduleId])).cron, WEEKDAY_CRON);
    });

    it("`schedule delete` takes it down, and the list no longer shows it", async () => {
      assert.ok(scheduleId, "no schedule was created");
      const deleted = await run([...SCHEDULE, "delete", fleetId, scheduleId, JSON_FLAG]);
      assert.equal(deleted.code, 0, `schedule delete failed: ${deleted.stderr}`);
      assert.equal(Schema.decodeUnknownSync(DeletedSchema)(JSON.parse(deleted.stdout.trim())).schedule_id,
        scheduleId);
      scheduleDeleted = true;

      // The row goes only once the scheduler confirms the removal, so a row
      // still listed here is a registration the scheduler kept.
      assert.ok((await listed()).every((schedule) => schedule.schedule_id !== scheduleId),
        `schedule list still shows ${scheduleId} after its delete`);
      const gone = await run([...SCHEDULE, "show", fleetId, scheduleId, JSON_FLAG]);
      assert.equal(gone.code, EXIT_CODE.ServerError, `a deleted schedule still answered: ${gone.stdout}`);
      assert.equal(refusalCode(gone), CODE_SCHEDULE_NOT_FOUND);
    });
  });
}
