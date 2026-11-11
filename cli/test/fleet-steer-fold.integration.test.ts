// Where the daemon placed a steer: a folded send says it joined the run and
// waits for its own reply on its own event, a queued send says so, and --json
// carries both fields. A daemon that predates the field reads as `started`.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { steerEffectFromArgs } from "../src/commands/fleet/steer.ts";
import { KIND_EVENT_COMPLETE } from "../src/commands/fleet/steer-events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { exitToCliError } from "../src/lib/cli-error-render.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { FLEET_ID, EVENT_ID, streamFrom, nullOutput, makeRecorder, makeLayer, eventStream } from "./helpers-fleet-steer.ts";

const RUN_EVENT_ID = "0190d5c2-7e1a-7000-8000-00000000e00f";

// The run reports: its own event completes first, then the folded message's,
// answered by the same reply.
const runCompletes = [
  { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: RUN_EVENT_ID, status: EVENT_STATUS.PROCESSED } },
  { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
] satisfies Parameters<StreamGetCallback>[0][];

const placed = (disposition?: string, folded?: string) => <T>() =>
  ({
    event_id: EVENT_ID,
    ...(disposition === undefined ? {} : { disposition }),
    ...(folded ? { folded_into_event_id: folded } : {}),
  }) as T;

const ownCompletes = [
  { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
] satisfies Parameters<StreamGetCallback>[0][];

describe("steer — disposition", () => {
  test("a folded send says it joined the run and settles on its own event", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "and also this", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(runCompletes),
      }).pipe(Effect.provide(makeLayer(rec, placed("folded", RUN_EVENT_ID)))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout).toContain("added to the current run");
    expect(rec.stdout.some((line) => line.includes(`event ${EVENT_ID}`))).toBe(true);
    expect(rec.stdout.some((line) => line.includes(`event ${RUN_EVENT_ID}`))).toBe(false);
  });

  test("a queued send says it waits behind the current run", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "next", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(ownCompletes),
      }).pipe(Effect.provide(makeLayer(rec, placed("queued")))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout).toContain("queued behind the current run");
  });

  test("--json carries the disposition and the run a fold joined", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "and also this", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(runCompletes),
      }).pipe(Effect.provide(makeLayer(rec, placed("folded", RUN_EVENT_ID), true))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const json = rec.stdout.find((line) => line.startsWith("{"));
    const parsed = JSON.parse(json ?? "{}") as Record<string, unknown>;
    expect(parsed["disposition"]).toBe("folded");
    expect(parsed["folded_into_event_id"]).toBe(RUN_EVENT_ID);
    expect(parsed["event_id"]).toBe(EVENT_ID);
  });

  test("a send the daemon says started carries that in --json", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "next", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(ownCompletes),
      }).pipe(Effect.provide(makeLayer(rec, placed("started"), true))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const json = rec.stdout.find((line) => line.startsWith("{"));
    const parsed = JSON.parse(json ?? "{}") as Record<string, unknown>;
    expect(parsed["disposition"]).toBe("started");
    expect(parsed).not.toHaveProperty("folded_into_event_id");
  });

  test.each([["a started send", "started"], ["a daemon that sends no disposition", undefined]])(
    "%s prints no placement line",
    async (_name, disposition) => {
      const rec = makeRecorder();
      const exit = await Effect.runPromiseExit(
        steerEffectFromArgs(FLEET_ID, "next", {}, {
          stdin: streamFrom([], false),
          stdout: nullOutput(),
          streamGet: eventStream(ownCompletes),
        }).pipe(Effect.provide(makeLayer(rec, placed(disposition)))),
      );
      expect(Exit.isSuccess(exit)).toBe(true);
      expect(rec.stdout.join("\n")).not.toContain("current run");
      expect(rec.stdout.join("\n")).not.toContain("queued behind");
    },
  );

  test("a disposition this CLI does not know fails and warns the message may still run", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "next", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(ownCompletes),
      }).pipe(Effect.provide(makeLayer(rec, placed("later")))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    const error = exitToCliError(exit);
    expect(error).toMatchObject({ code: "BAD_RESPONSE", suggestion: expect.stringContaining("may still run") });
  });
});
