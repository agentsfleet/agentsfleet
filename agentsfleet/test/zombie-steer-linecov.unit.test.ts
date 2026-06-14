// Regression coverage for src/commands/zombie_steer.ts fallback behavior.
//
// These tests pin down the reachable behavior that bypasses direct
// Server-Sent Events (SSE) transport-error rendering:
//
//   * any `sse_error` produced by tailEventStream is overwritten by the
//     fallback poll in steerTurnEffect before renderOutcome inspects the
//     outcome. We prove the poll recovery path renders instead.
//
//   * runTurn failures are still surfaced as their original CliError message
//     so the prompt loop can continue after one failed turn.

import { describe, expect, test, setSystemTime } from "bun:test";
import { Effect, Exit } from "effect";

import { steerEffectFromArgs } from "../src/commands/zombie_steer.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
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
      steerEffectFromArgs(ZOMBIE_ID, SINGLE_MESSAGE, {}, {
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
        steerEffectFromArgs(ZOMBIE_ID, SINGLE_MESSAGE, {}, {
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
      steerEffectFromArgs(ZOMBIE_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["first\nsecond\n"], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          { id: null, type: "event_complete", data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } },
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
      steerEffectFromArgs(ZOMBIE_ID, SINGLE_MESSAGE, {}, {
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
