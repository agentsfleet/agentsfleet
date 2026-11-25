// The wake-ups journey, from a terminal: night-owl schedules itself and adds a
// signed webhook, and a person reads and removes both.
//
// The fleet's name, its messages, its cron expression and its webhook's source
// and secret are `tests/fixtures/fleets/night-owl.json`. The journey reads the
// rows the fleet's tools wrote; it sends no delivery and waits for no wake.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { entry, field, primeFleet as primeFixture, said, wake } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession, storeSecret } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, steer } from "./fixtures/prime-turns.ts";
import { SCHEDULE_STATUS } from "../../src/commands/schedule/read.ts";

const fixture = primeFixture("night-owl");
const scheduleAsk = entry(fixture.follow_ups, 0, "follow-up");
const webhookAsk = entry(fixture.follow_ups, 1, "follow-up");
const cron = field(wake(fixture, "schedule").cron, "schedule cron");
const webhook = wake(fixture, "webhook");
const webhookSource = field(webhook.source, "webhook source");
const webhookSecret = field(webhook.secret_name, "webhook secret name");
const WORKSPACE = `fixture-cli-${fixture.journey}`;

const SETUP_TIMEOUT_MS = 360_000;
const STEP_TIMEOUT_MS = 300_000;
const SECRET_BYTES = 24;
// How a webhook from a source the daemon has no recipe for signs its body: the
// trigger tool takes the scheme and header, and the fixture's sentence names
// neither, so the journey says which.
const VERIFIER_SCHEME = "body-hex-sha256";
const VERIFIER_HEADER = "x-hub-signature-256";
// `af_wire::trigger_verb::TriggerKind::Webhook`.
const TRIGGER_KIND_WEBHOOK = "webhook";
const SCHEDULE = ["fleet", "schedule"] as const;
const TRIGGERS = ["fleet", "triggers"] as const;

const ScheduleSchema = Schema.Struct({
  schedule_id: Schema.String,
  fleet_id: Schema.String,
  cron: Schema.String,
  status: Schema.String,
});
const SchedulePageSchema = Schema.Struct({ schedules: Schema.Array(ScheduleSchema) });
const ScheduleDeletedSchema = Schema.Struct({ deleted: Schema.Literal(true), schedule_id: Schema.String });
const TriggerSchema = Schema.Struct({
  id: Schema.String,
  kind: Schema.String,
  source: Schema.String,
  url: Schema.NullOr(Schema.String),
});
const TriggerPageSchema = Schema.Struct({ items: Schema.Array(TriggerSchema) });
const TriggerDeletedSchema = Schema.Struct({ deleted: Schema.Literal(true), trigger_id: Schema.String });

if (!isLive) {
  describe("prime-night-owl-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("night-owl: a fleet adds what wakes it, and a person reads and removes it", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    let fleetId = "";
    let scheduleId = "";
    let triggerId = "";

    const schedules = async () =>
      Schema.decodeUnknownSync(SchedulePageSchema)(await session.json([...SCHEDULE, "list", fleetId])).schedules;
    const triggers = async () =>
      Schema.decodeUnknownSync(TriggerPageSchema)(await session.json([...TRIGGERS, "list", fleetId])).items;

    beforeAll(async () => {
      // The webhook's signing secret: a real vault entry with a value of this
      // run's. The daemon checks the vault holds the name before it adds the row.
      await storeSecret(session, webhookSecret, { webhook_secret: crypto.randomBytes(SECRET_BYTES).toString("hex") });
      const created = await primeFleet(session, fixture);
      fleetId = created.fleet_id;
      await awaitTurn(session, fleetId, processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
      // A schedule still registered when its fleet goes leaves the scheduler
      // calling a fleet nothing resolves, so teardown removes any it finds first.
      session.onClose(async () => {
        for (const row of await schedules()) await session.json([...SCHEDULE, "delete", fleetId, row.schedule_id]);
      });
    }, SETUP_TIMEOUT_MS);

    it("the schedule the fleet adds for itself shows in `fleet schedule list`", async () => {
      assert.ok(fleetId, "the fleet was not created");
      assert.deepEqual(await schedules(), [], "a fleet starts with no schedule");
      await steer(session, fleetId, said(scheduleAsk, `Use the cron expression ${cron}.`));

      const listed = await schedules();
      const mine = listed.filter((row) => row.cron === cron);
      assert.equal(mine.length, 1, `expected one schedule on ${cron}; fleet schedule list shows ${JSON.stringify(listed)}`);
      const added = entry(mine, 0, "schedule");
      assert.equal(added.fleet_id, fleetId);
      assert.equal(added.status, SCHEDULE_STATUS.active);
      scheduleId = added.schedule_id;
    }, STEP_TIMEOUT_MS);

    it("a person deletes the fleet's schedule, and the list no longer shows it", async () => {
      assert.ok(scheduleId, "the fleet added no schedule");
      const deleted = Schema.decodeUnknownSync(ScheduleDeletedSchema)(
        await session.json([...SCHEDULE, "delete", fleetId, scheduleId]));
      assert.equal(deleted.schedule_id, scheduleId);
      assert.ok((await schedules()).every((row) => row.schedule_id !== scheduleId),
        `fleet schedule list still shows ${scheduleId} after its delete`);
    });

    it("the webhook the fleet adds shows in `fleet triggers list` with the URL deliveries go to", async () => {
      assert.ok(fleetId, "the fleet was not created");
      assert.deepEqual(await triggers(), [], "a fleet starts with no trigger");
      await steer(session, fleetId,
        said(webhookAsk, `Verify deliveries with the ${VERIFIER_SCHEME} scheme on the ${VERIFIER_HEADER} header.`));

      const listed = await triggers();
      const mine = listed.filter((row) => row.kind === TRIGGER_KIND_WEBHOOK && row.source === webhookSource);
      assert.equal(mine.length, 1, `expected one ${webhookSource} webhook; fleet triggers list shows ${JSON.stringify(listed)}`);
      const added = entry(mine, 0, "trigger");
      assert.ok(added.url?.endsWith(`/v1/webhooks/${fleetId}/${webhookSource}`),
        `the webhook's URL does not address this fleet and source: ${added.url}`);
      triggerId = added.id;
    }, STEP_TIMEOUT_MS);

    it("`fleet triggers delete` removes the webhook, and deleting it again changes nothing", async () => {
      assert.ok(triggerId, "the fleet added no webhook");
      const deleted = Schema.decodeUnknownSync(TriggerDeletedSchema)(
        await session.json([...TRIGGERS, "delete", fleetId, triggerId]));
      assert.equal(deleted.trigger_id, triggerId);
      assert.deepEqual(await triggers(), [], "the deleted webhook is still listed");

      const again = Schema.decodeUnknownSync(TriggerDeletedSchema)(
        await session.json([...TRIGGERS, "delete", fleetId, triggerId]));
      assert.equal(again.trigger_id, triggerId);
      assert.deepEqual(await triggers(), []);
    });
  });
}
