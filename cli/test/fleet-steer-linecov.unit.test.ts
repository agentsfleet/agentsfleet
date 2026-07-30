// Regression coverage for src/commands/fleet_steer.ts fallback behavior.
//
// These tests pin down the reachable behavior that bypasses direct
// Server-Sent Events (SSE) transport-error rendering:
//
//   * any `sse_error` produced by the event tail is overwritten by the
//     fallback poll in steerTurnEffect before renderOutcome inspects the
//     outcome. We prove the poll recovery path renders instead.
//
//   * runTurn failures are still surfaced as their original CliError message
//     so the prompt loop can continue after one failed turn.
//
//   * frames arriving before the POST names the event wait in the bounded
//     pre-id buffer: replay order, foreign-event drop, overflow, and abort
//     inside that window.

import { describe, expect, test, setSystemTime } from "bun:test";
import { Effect, Exit } from "effect";

import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import {
  KIND_CHUNK,
  KIND_EVENT_COMPLETE,
  PRE_ID_BUFFER_MAX_BYTES,
  PRE_ID_BUFFER_MAX_FRAMES,
} from "../src/commands/fleet_steer_events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { SIGINT } from "../src/constants/signals.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
import {
  FLEET_ID,
  EVENT_ID,
  OTHER_EVENT_ID,
  streamFrom,
  nullOutput,
  makeRecorder,
  makeLayer,
  eventStream,
} from "./fleet-steer.integration.test.ts";

const POST = "POST";
const SINGLE_MESSAGE = "go";
// renderOutcome's dead arm would emit this prefix for an sse_error outcome.
const SSE_ERROR_RENDER_PREFIX = "message failed: sse_error";

type StreamGetFn = typeof import("../src/lib/sse.ts").streamGet;

const throwingStream: StreamGetFn = async (): Promise<void> => {
  throw new Error("connection refused");
};
const silentStream: StreamGetFn = async (): Promise<void> => { /* no frames */ };

const isPost = (input: HttpRequestInput): boolean => input.method === POST;

const postedEvent = <T>(): T => ({ event_id: EVENT_ID } as T);

describe("steer — sse_error never renders because the poll overwrites it", () => {
  test("an SSE transport error is rendered as the recovered terminal status, not as sse_error", async () => {
    const rec = makeRecorder();
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (isPost(input)) return postedEvent<T>();
      // Recovery poll finds the event already PROCESSED.
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };

    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: throwingStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );

    // The poll recovered → success, and the success line carries the status —
    // the sse_error render arm was bypassed entirely.
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.requests.some((r) => !isPost(r))).toBe(true);
    expect(rec.stdout.join("\n")).toContain(`${EVENT_ID} ${EVENT_STATUS.PROCESSED}`);
    expect(rec.stderr.join("\n")).not.toContain(SSE_ERROR_RENDER_PREFIX);
  });

  test("when the poll also yields nothing, the outcome renders as timeout — still not sse_error", async () => {
    const rec = makeRecorder();
    let firstPoll = true;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (isPost(input)) return postedEvent<T>();
      if (firstPoll) {
        firstPoll = false;
        setSystemTime(Date.now() + 120_000); // jump past the 60s poll deadline
      }
      return { items: [] } as T;
    };

    try {
      const exit = await Effect.runPromiseExit(
        steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
          stdin: streamFrom([], false),
          stdout: nullOutput(),
          streamGet: throwingStream,
        }).pipe(Effect.provide(makeLayer(rec, httpReply))),
      );
      // sse_error was overwritten by timeout; the timeout arm renders, the
      // sse_error arm does not.
      expect(Exit.isFailure(exit)).toBe(true);
      expect(rec.stderr.some((m) => m.includes("still in flight"))).toBe(true);
      expect(rec.stderr.join("\n")).not.toContain(SSE_ERROR_RENDER_PREFIX);
    } finally {
      setSystemTime();
    }
  }, 10_000);
});

describe("steer — onTurnError classifies CliError turn failures via the _tag arm", () => {
  test("a failing REPL turn renders the original CliError, never a synthesized UnexpectedError", async () => {
    const rec = makeRecorder();
    // First POST omits event_id → steerTurnEffect fails with a ServerError
    // (a tagged CliError). runTurn throws exitToCliError(exit) → onTurnError
    // sees a `_tag`-bearing cause → renderCliError prints its detail.
    let postCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (isPost(input)) {
        postCount += 1;
        if (postCount === 1) return {} as T; // no event_id → ServerError
        return postedEvent<T>();
      }
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };

    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["first\nsecond\n"], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          { id: null, type: KIND_EVENT_COMPLETE, data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
        ]),
        signalSource: new ReplSignalEmitter(),
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );

    // Loop survives the first (failed) turn and runs the second.
    expect(Exit.isSuccess(exit)).toBe(true);
    // The rendered error is the genuine ServerError detail — proof the _tag
    // true arm fired, not the UnexpectedError else arm.
    const renderedErrors = rec.stderr.join("\n");
    expect(renderedErrors).toContain("messages response missing event_id");
    expect(renderedErrors).not.toContain("report this with the command you ran");
    // Second turn still posted + completed.
    expect(rec.requests.filter(isPost)).toHaveLength(2);
  });
});

describe("steer — single-shot SSE error path is shielded by the recovery poll", () => {
  test("a silent stream plus a recovering poll renders the terminal status without an error", async () => {
    const rec = makeRecorder();
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (isPost(input)) return postedEvent<T>();
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };

    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: silentStream,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.join("\n")).toContain(`${EVENT_ID} ${EVENT_STATUS.PROCESSED}`);
    expect(rec.stderr.join("\n")).not.toContain(SSE_ERROR_RENDER_PREFIX);
  });
});

// ── Pre-id buffer: replay order, foreign drop, bounds ─────────────────────

type SteerFrame = Parameters<typeof eventStream>[0][number];

const chunkFrame = (eventId: string, text: string): SteerFrame =>
  ({ id: null, type: KIND_CHUNK, data: { event_id: eventId, text } });
const completeFrame = (eventId: string): SteerFrame =>
  ({ id: null, type: KIND_EVENT_COMPLETE, data: { event_id: eventId, status: EVENT_STATUS.PROCESSED } });

describe("steer — pre-id frames buffer until the POST names the event", () => {
  test("test_pre_id_frames_replayed_in_order", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          chunkFrame(EVENT_ID, "first words"),
          chunkFrame(EVENT_ID, "second words"),
          completeFrame(EVENT_ID),
        ]),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const first = rec.stdout.findIndex((l) => l.includes("first words"));
    const second = rec.stdout.findIndex((l) => l.includes("second words"));
    expect(first).toBeGreaterThanOrEqual(0);
    expect(second).toBeGreaterThan(first);
  });

  test("test_foreign_event_frames_dropped", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          chunkFrame(OTHER_EVENT_ID, "foreign words"),
          chunkFrame(EVENT_ID, "our words"),
          completeFrame(EVENT_ID),
        ]),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("foreign words"))).toBe(false);
    expect(rec.stdout.some((l) => l.includes("our words"))).toBe(true);
  });

  test("test_pre_id_buffer_bounded", async () => {
    // Frame-count bound: overflow drops oldest; the tail still completes.
    const rec = makeRecorder();
    const overflowBy = 3;
    const frames = Array.from(
      { length: PRE_ID_BUFFER_MAX_FRAMES + overflowBy },
      (_, i) => chunkFrame(EVENT_ID, `line ${i}`),
    );
    frames.push(completeFrame(EVENT_ID));
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream(frames),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.endsWith("line 0"))).toBe(false);
    expect(
      rec.stdout.some((l) => l.endsWith(`line ${PRE_ID_BUFFER_MAX_FRAMES + overflowBy - 1}`)),
    ).toBe(true);

    // Byte bound: two half-cap frames exceed the byte cap; oldest drops.
    const rec2 = makeRecorder();
    const half = "x".repeat(PRE_ID_BUFFER_MAX_BYTES / 2);
    const exit2 = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          chunkFrame(EVENT_ID, `first-giant ${half}`),
          chunkFrame(EVENT_ID, `second-giant ${half}`),
          completeFrame(EVENT_ID),
        ]),
      }).pipe(Effect.provide(makeLayer(rec2))),
    );
    expect(Exit.isSuccess(exit2)).toBe(true);
    expect(rec2.stdout.some((l) => l.includes("first-giant"))).toBe(false);
    expect(rec2.stdout.some((l) => l.includes("second-giant"))).toBe(true);
  });
});

// ── Abort inside the pre-id window ────────────────────────────────────────

describe("steer — abort inside the pre-id window", () => {
  test("test_abort_in_pre_id_window", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    const httpReply = <T>(_input: HttpRequestInput): T => {
      // The SIGINT lands while the POST is in flight — after the stream
      // opened, before the response names the event.
      signalSource.emit(SIGINT);
      return postedEvent<T>();
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["hi\n"], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          chunkFrame(EVENT_ID, "early words"),
          completeFrame(EVENT_ID),
        ]),
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("early words"))).toBe(false);
  });
});
