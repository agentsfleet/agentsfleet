import { describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { Writable } from "node:stream";
import { makeStdioOutput, OUTPUT_FORMAT } from "../../src/services/output.ts";
import { ChatActions } from "../../src/commands/chat/actions.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { SetupSession } from "../../src/commands/chat/setup-session.ts";
import { isPartial, renderApplied, renderFleet, renderMessage, renderReview, schedulesOwed } from "../../src/commands/chat/render.ts";
import { AppliedSchema, decode, FleetSchema } from "../../src/commands/chat/protocol.ts";
import { DRAFT, ACCEPTED, FAILED_SCHEDULE, FLEET, FLEET_ID, REVIEW, MESSAGE, MODEL, PRIVATE_TEXT, SCHEDULE_RETRY,
  SYNC_UNCONFIRMED_KIND, SYNCED_SCHEDULE, SYNCING_SCHEDULE, UNKNOWN_SYNC_SCHEDULE, withApi } from "./fixture.ts";

function streams(format: typeof OUTPUT_FORMAT[keyof typeof OUTPUT_FORMAT] = OUTPUT_FORMAT.json) {
  const stdout: string[] = []; const stderr: string[] = [];
  const output = makeStdioOutput({format,
    stdout: new Writable({write(value, _encoding, done) {stdout.push(String(value)); done();}}),
    stderr: new Writable({write(value, _encoding, done) {stderr.push(String(value)); done();}})});
  return {stdout, stderr, output};
}

describe("scriptable chat output", () => {
  test("messages, reviews and saved fleets each occupy one complete JSON line", async () => {
    const {output, stdout} = streams();
    const message = {...MESSAGE, text: "private\ntext with { braces } and quotes \""};
    await Effect.runPromise(Effect.gen(function* () {
      yield* renderMessage(output, message); yield* renderReview(output, REVIEW); yield* renderFleet(output, FLEET);
    }));
    const lines = stdout.join("").trim().split("\n");
    expect(lines).toHaveLength(3);
    expect(lines.map((line) => JSON.parse(line))).toEqual([message, REVIEW, FLEET]);
  });
  test("an ordinary send emits a structured acceptance receipt before durable messages", async () => {
    await withApi((request) => Response.json(new URL(request.url).pathname.endsWith("/messages")
      ? {items: [MESSAGE], next_cursor: null} : {...DRAFT, accepted: ACCEPTED}), async (transport) => {
      const {output, stdout} = streams();
      const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}),
        new SetupSession(transport), output, () => Effect.void);
      await Effect.runPromise(actions.message(PRIVATE_TEXT));
      const values = stdout.join("").trim().split("\n").map((line): unknown => JSON.parse(line));
      expect(values[0]).toEqual({kind: "chat_accepted", draft_id: DRAFT.id, sequence: "1"});
      expect(values).toContainEqual(MESSAGE);
    });
  });
});

describe("schedule registration outcomes", () => {
  // The published `View` carries more than the client consumes; the outcome fields must survive decoding.
  const view = {...FAILED_SCHEDULE, cron: "0 9 * * *", timezone: "UTC", message: "triage", source: "trigger",
    once: false, status: "active", etag: '"schedule-1"', created_at: "1970-01-01T00:00:00.001Z", updated_at: "1970-01-01T00:00:00.001Z"};

  test("decoding a fleet or an applied revision keeps each schedule's sync outcome", async () => {
    const fleet = await Effect.runPromise(decode(FleetSchema, {...FLEET, schedules: [view]}));
    expect(fleet.schedules).toEqual([FAILED_SCHEDULE]);
    const applied = await Effect.runPromise(decode(AppliedSchema, {fleet_id: FLEET_ID, setup_revision: "2",
      setup_etag: '"setup-2"', etag: FLEET.etag, schedules: [view]}));
    expect(applied.schedules).toEqual([FAILED_SCHEDULE]);
  });

  test("an active fleet with a failed schedule is partial; synced schedules leave it complete", () => {
    expect(isPartial({...FLEET, schedules: [SYNCED_SCHEDULE, FAILED_SCHEDULE]})).toBe(true);
    expect(isPartial({...FLEET, schedules: [SYNCED_SCHEDULE]})).toBe(false);
  });

  test("a schedule left syncing, or in a status this client does not know, is owed rather than synced", async () => {
    const fleet = await Effect.runPromise(decode(FleetSchema, {...FLEET, schedules: [UNKNOWN_SYNC_SCHEDULE]}));
    expect(fleet.schedules).toEqual([UNKNOWN_SYNC_SCHEDULE]);
    expect(isPartial(fleet)).toBe(true);
    expect(isPartial({...FLEET, schedules: [SYNCING_SCHEDULE]})).toBe(true);
    expect(schedulesOwed([SYNCED_SCHEDULE, UNKNOWN_SYNC_SCHEDULE])).toBe(true);
    const {output, stderr} = streams();
    await Effect.runPromise(renderFleet(output, fleet));
    expect(JSON.parse(stderr.join(""))).toEqual({kind: SYNC_UNCONFIRMED_KIND, fleet_id: FLEET_ID,
      schedule_id: UNKNOWN_SYNC_SCHEDULE.schedule_id, last_error: null, retry: SCHEDULE_RETRY});
  });

  test("a terminal names the failed schedule, its error and the retry command", async () => {
    const {output, stderr} = streams(OUTPUT_FORMAT.text);
    await Effect.runPromise(renderFleet(output, {...FLEET, schedules: [SYNCED_SCHEDULE, FAILED_SCHEDULE]}));
    const warned = stderr.join("");
    expect(warned).toContain(`Retry with: ${SCHEDULE_RETRY}`);
    expect(warned).toContain(String(FAILED_SCHEDULE.last_error));
    expect(warned.split(SCHEDULE_RETRY)).toHaveLength(2);
  });

  test("an applied revision with a failed schedule warns without an error when none was recorded", async () => {
    const {output, stderr} = streams(OUTPUT_FORMAT.text);
    const {last_error: _omitted, ...bare} = FAILED_SCHEDULE;
    await Effect.runPromise(renderApplied(output, {fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-2"',
      etag: FLEET.etag, schedules: [bare]}));
    expect(stderr.join("")).toContain(`will not fire. Retry with: ${SCHEDULE_RETRY}`);
  });
});
