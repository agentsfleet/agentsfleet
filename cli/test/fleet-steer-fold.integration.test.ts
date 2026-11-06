// Where the daemon placed a steer (M242_002): a fold follows the live run's
// event, a queued send says so, and --json carries both fields.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import { KIND_EVENT_COMPLETE } from "../src/commands/fleet_steer_events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { FLEET_ID, EVENT_ID, streamFrom, nullOutput, makeRecorder, makeLayer, eventStream } from "./helpers-fleet-steer.ts";

const RUN_EVENT_ID = "0190d5c2-7e1a-7000-8000-00000000e00f";

const runCompletes = [
  { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: RUN_EVENT_ID, status: EVENT_STATUS.PROCESSED } },
] satisfies Parameters<StreamGetCallback>[0][];

const placed = (disposition: string, folded?: string) => <T>() =>
  ({ event_id: EVENT_ID, disposition, ...(folded ? { folded_into_event_id: folded } : {}) }) as T;

describe("steer — disposition", () => {
  test("a folded send says it joined the run and settles on the run's event", async () => {
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
    expect(rec.stdout.some((line) => line.includes(RUN_EVENT_ID))).toBe(true);
  });

  test("a queued send says it waits behind the current run", async () => {
    const rec = makeRecorder();
    const ownCompletes = [
      { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
    ] satisfies Parameters<StreamGetCallback>[0][];
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
    expect(parsed["event_id"]).toBe(RUN_EVENT_ID);
  });
});
