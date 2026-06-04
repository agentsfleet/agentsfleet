// Integration + unit coverage for src/commands/zombie_steer.ts (part 2).
// Targets error paths, poll loop, and REPL tryPromise catch:
// lines 139-141, 213, 216, 235-238, 255-261, 312-314, 319-324.
//
// Uses setSystemTime (bun:test) to skip the 60s poll timeout in tests
// that need pollEventTerminal to return "timeout" quickly.

import { describe, expect, test, setSystemTime } from "bun:test";
import { Cause, Effect, Exit, Option } from "effect";

import { steerEffectFromArgs } from "../src/commands/zombie_steer.ts";
import { UnexpectedError } from "../src/errors/index.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
// Shared fixtures + mocked-layer config — single source of truth with the
// part-1 steer suite so the two files cannot drift apart (makeLayer closes
// over the part-1 constants, so only the values used directly here are imported).
import {
  ZOMBIE_ID,
  EVENT_ID,
  streamFrom,
  nullOutput,
  makeRecorder,
  makeLayer,
  eventStream,
} from "./zombie-steer.integration.test.ts";

const POST = "POST";

type StreamGetFn = typeof import("../src/lib/sse.ts").streamGet;

const silentStream: StreamGetFn = async (): Promise<void> => { /* no events → sse_disconnected */ };

// ── SSE error path (lines 139-141) ────────────────────────────────────────

describe("steer — SSE error path (lines 139-141)", () => {
  test("streamGet throwing an Error is caught as sse_error; poll recovers", async () => {
    const rec = makeRecorder();
    const throwingStream = async (): Promise<void> => { throw new Error("connection refused"); };
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(ZOMBIE_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: throwingStream as StreamGetFn,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // Discriminator: a non-POST request (the recovery poll GET) proves the
    // sse_error branch ran, not a stream that completed on its own.
    expect(rec.requests.some((r) => r.method !== POST)).toBe(true);
  });

  test("streamGet throwing non-Error is caught and stringified (line 141)", async () => {
    const rec = makeRecorder();
    // oxlint-disable-next-line no-throw-literal -- fixture: simulate a non-Error rejection from the stream
    const throwingStream = async (): Promise<void> => { throw "raw string error"; };
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(ZOMBIE_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: throwingStream as StreamGetFn,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // Discriminator: the recovery poll GET ran after the stringified error,
    // so the sse_error catch executed rather than a clean stream completion.
    expect(rec.requests.some((r) => r.method !== POST)).toBe(true);
  });
});

// ── Poll terminal match (lines 213, 216) ──────────────────────────────────

describe("steer — poll terminal match (lines 213, 216)", () => {
  test("poll finds matching terminal event after empty response (line 213)", async () => {
    const rec = makeRecorder();
    let pollCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      pollCount += 1;
      if (pollCount >= 2) {
        return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
      }
      return { items: [] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(ZOMBIE_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: silentStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(pollCount).toBeGreaterThanOrEqual(2);
  });

  test("poll aborted signal exits loop immediately (line 216)", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    let pollCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      pollCount += 1;
      signalSource.emit("SIGINT" as const);
      return { items: [] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(ZOMBIE_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["hi\n"], false),
        stdout: nullOutput(),
        streamGet: silentStream,
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(pollCount).toBe(1);
  });
});

// ── renderOutcome timeout path (lines 235-238, 255-261) ───────────────────
// Uses setSystemTime to advance Date.now() past the 60s deadline after one
// poll iteration, making pollEventTerminal return "timeout" without waiting.

describe("steer — renderOutcome timeout path (lines 235-238, 255-261)", () => {
  test("poll timeout prints 'still in flight' error and fails with ConfigError", async () => {
    const rec = makeRecorder();
    let firstPoll = true;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      if (firstPoll) {
        firstPoll = false;
        setSystemTime(Date.now() + 120_000); // push past the 60s deadline
      }
      return { items: [] } as T;
    };
    try {
      const exit = await Effect.runPromiseExit(
        steerEffectFromArgs(ZOMBIE_ID, "go", {}, {
          stdin: streamFrom([], false),
          stdout: nullOutput(),
          streamGet: silentStream,
        }).pipe(Effect.provide(makeLayer(rec, httpReply))),
      );
      expect(Exit.isFailure(exit)).toBe(true);
      expect(rec.stderr.some((m) => m.includes("still in flight"))).toBe(true);
    } finally {
      setSystemTime(); // restore real clock
    }
  }, 10_000);
});

// ── REPL tryPromise catch (lines 319-324) ─────────────────────────────────
// onTurnError calls Effect.runPromise(renderCliError(...)). If Output.error
// returns a failing Effect, runPromise rejects with a FiberFailure that has
// no "_tag" in the CliError sense. That rejection propagates through
// runSteerRepl → tryPromise.catch → lines 319-324.

describe("steer — REPL tryPromise catch (lines 319-324)", () => {
  test("Output.error fail in onTurnError causes tryPromise catch with UnexpectedError", async () => {
    const rec = makeRecorder();
    let errorCallCount = 0;

    const errorOverride = (_m: string) => {
      errorCallCount += 1;
      if (errorCallCount === 1) {
        // First error call (from onTurnError's renderCliError) fails the Effect.
        return Effect.fail(new Error("output-layer-crash") as unknown as never);
      }
      return Effect.sync(() => { rec.stderr.push(_m); });
    };

    // First POST returns no event_id → ServerError → onTurnError invoked.
    let postCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) {
        postCount += 1;
        if (postCount === 1) return {} as T; // triggers ServerError (no event_id)
        return { event_id: EVENT_ID } as T;
      }
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };

    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(ZOMBIE_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["first\nsecond\n"], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
        ]),
        signalSource: new ReplSignalEmitter(),
      }).pipe(Effect.provide(makeLayer(rec, httpReply, false, { error: errorOverride }))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    // The catch must classify the non-CliError rejection as UnexpectedError,
    // not merely fail — that is the branch this test name claims to cover.
    const err = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(err).toBeInstanceOf(UnexpectedError);
  });
});
