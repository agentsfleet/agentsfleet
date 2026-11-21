// Stream error handling and REPL failures for fleet steer.
import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Option, Redacted } from "effect";

import { steerEffectFromArgs } from "../src/commands/fleet/steer.ts";
import {
  KIND_TURN_COMPLETE,
  STATUS_SSE_DISCONNECTED,
  openTurnTail,
} from "../src/commands/fleet/steer-turns.ts";
import { UnexpectedError } from "../src/errors/index.ts";
import { TURN_STATUS } from "../src/constants/turn-status.ts";
import { type HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
// Shared fixtures + mocked-layer config — single source of truth with the
// part-1 steer suite so the two files cannot drift apart (makeLayer closes
// over the part-1 constants, so only the values used directly here are imported).
import {
  CALL_POST,
  CALL_STREAM_OPEN,
  FLEET_ID,
  TURN_ID,
  OTHER_TURN_ID,
  POST,
  TOKEN,
  streamFrom,
  nullOutput,
  makeRecorder,
  makeLayer,
  eventStream,
} from "./helpers-fleet-steer.ts";

type StreamGetFn = typeof import("../src/lib/sse.ts").streamGet;

// ── SSE error path (lines 139-141) ────────────────────────────────────────

describe("steer — SSE error path (lines 139-141)", () => {
  test("stream frames for a different turn_id are ignored", async () => {
    const rec = makeRecorder();
    const otherTurnStream: StreamGetFn = async (_url, _headers, cb) => {
      cb({
        id: null,
        type: KIND_TURN_COMPLETE,
        data: {
          turn_id: OTHER_TURN_ID,
          status: TURN_STATUS.PROCESSED,
        },
      });
    };
    const handle = await Effect.runPromise(
      openTurnTail(
        "ws_test",
        FLEET_ID,
        Redacted.make(TOKEN),
        otherTurnStream,
      ).pipe(Effect.provide(makeLayer(rec))),
    );
    handle.deliverTurnId(TURN_ID);
    const outcome = await handle.awaitOutcome();
    expect(outcome.kind).toBe(STATUS_SSE_DISCONNECTED);
    expect(rec.stdout).toEqual([]);
  });

  test("streamGet throwing an Error is caught as sse_error; poll recovers", async () => {
    const rec = makeRecorder();
    const throwingStream = async (): Promise<void> => { throw new Error("connection refused"); };
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { turn_id: TURN_ID } as T;
      return { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
      if (input.method === POST) return { turn_id: TURN_ID } as T;
      return { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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

    // First POST returns no turn_id → ServerError → onTurnError invoked.
    let postCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) {
        postCount += 1;
        if (postCount === 1) return {} as T; // triggers ServerError (no turn_id)
        return { turn_id: TURN_ID } as T;
      }
      return { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } as T;
    };

    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["first\nsecond\n"], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          { id: null, type: KIND_TURN_COMPLETE, data: { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } },
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

// ── Subscribe-first failure paths degrade to today's behaviour ────────────

describe("steer — subscribe-first failure paths", () => {
  test("test_stream_open_failure_degrades_to_poll", async () => {
    const rec = makeRecorder();
    const calls: string[] = [];
    const failingStream: StreamGetFn = async (): Promise<void> => {
      calls.push(CALL_STREAM_OPEN);
      throw new Error("connection refused");
    };
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) {
        calls.push(CALL_POST);
        return { turn_id: TURN_ID } as T;
      }
      return { turn_id: TURN_ID, status: TURN_STATUS.PROCESSED } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: failingStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    // The broken stream never blocks the send: the POST still fires, the
    // terminal poll recovers, and the outcome renders exactly as today.
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls).toEqual([CALL_STREAM_OPEN, CALL_POST]);
    expect(rec.requests.some((r) => r.method !== POST)).toBe(true);
    expect(rec.stdout.join("\n")).toContain(`${TURN_ID} ${TURN_STATUS.PROCESSED}`);
  });

  test("test_post_failure_closes_stream", async () => {
    const rec = makeRecorder();
    let closeCount = 0;
    const trackedStream: StreamGetFn = (_url, _headers, _cb, options) =>
      new Promise<void>((resolveStream) => {
        options?.onOpen?.();
        options?.signal?.addEventListener(
          "abort",
          () => {
            closeCount += 1;
            resolveStream();
          },
          { once: true },
        );
      });
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) throw new Error("post exploded");
      return { items: [] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: trackedStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    // The failed send aborts the tail exactly once — no orphan connection —
    // and nothing was rendered from a turn that never produced an event.
    expect(Exit.isFailure(exit)).toBe(true);
    expect(closeCount).toBe(1);
    expect(rec.stdout).toEqual([]);
  });
});
