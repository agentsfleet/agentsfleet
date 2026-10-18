// Recovery polling and bounded timeout behavior for a submitted fleet event.
import { describe, expect, test, setSystemTime } from "bun:test";
import { Effect, Exit, Layer, Redacted } from "effect";
import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import { pollEventTerminal, STATUS_TIMEOUT } from "../src/commands/fleet_steer_events.ts";
import { ServerError } from "../src/errors/index.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { SIGINT } from "../src/constants/signals.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
import { FLEET_ID, EVENT_ID, POST, TOKEN, WS_ID, streamFrom, nullOutput, makeRecorder, makeLayer } from "./helpers-fleet-steer.ts";

type StreamGetFn = typeof import("../src/lib/sse.ts").streamGet;
const silentStream: StreamGetFn = async (): Promise<void> => {};

// ── Poll terminal match (lines 213, 216) ──────────────────────────────────

describe("steer — poll terminal match (lines 213, 216)", () => {
  test("poll finds matching terminal event after empty response (line 213)", async () => {
    const rec = makeRecorder();
    let pollCount = 0;
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return { event_id: EVENT_ID } as T;
      pollCount += 1;
      if (pollCount >= 2) {
        return { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED } as T;
      }
      return { items: [] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, "go", {}, {
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
      signalSource.emit(SIGINT);
      return { items: [] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["hi\n"], false),
        stdout: nullOutput(),
        streamGet: silentStream,
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(pollCount).toBe(1);
  });

  test("poll treats transient request failures as empty results", async () => {
    const ctrl = new AbortController();
    let requestCount = 0;
    const failingHttp = Layer.succeed(HttpClient, {
      request: () =>
        Effect.sync(() => {
          requestCount += 1;
          ctrl.abort();
        }).pipe(
          Effect.flatMap(() =>
            Effect.fail(
              new ServerError({
                detail: "temporary outage",
                suggestion: "retry",
                code: "UZ-INTERNAL-001",
                status: 503,
                requestId: null,
              }),
            ),
          ),
        ),
    });
    const outcome = await Effect.runPromise(
      pollEventTerminal(
        WS_ID,
        FLEET_ID,
        EVENT_ID,
        Redacted.make(TOKEN),
        ctrl.signal,
      ).pipe(Effect.provide(failingHttp)),
    );
    expect(outcome.kind).toBe(STATUS_TIMEOUT);
    expect(requestCount).toBe(1);
  });
});

describe("steer — renderOutcome timeout path", () => {
  test("poll timeout fails once, carrying 'still in flight' on the failure", async () => {
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
        steerEffectFromArgs(FLEET_ID, "go", {}, {
          stdin: streamFrom([], false),
          stdout: nullOutput(),
          streamGet: silentStream,
        }).pipe(Effect.provide(makeLayer(rec, httpReply))),
      );
      expect(Exit.isFailure(exit)).toBe(true);
      // One report of the stall, not two: the message rides the failure, and
      // nothing is written to standard error ahead of it.
      expect(JSON.stringify(exit)).toContain("still in flight");
      expect(rec.stderr.some((m) => m.includes("still in flight"))).toBe(false);
      // No pending gate in this scenario, so the ordinary suggestion stands.
      expect(JSON.stringify(exit)).toContain("agentsfleet events");
    } finally {
      setSystemTime(); // restore real clock
    }
  }, 10_000);
});

