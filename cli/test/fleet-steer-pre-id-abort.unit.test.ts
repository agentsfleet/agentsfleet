// Cancellation while the live tail is opening must never post or render a
// reply whose event identifier is still unknown.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";

import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import { KIND_CHUNK, KIND_EVENT_COMPLETE } from "../src/commands/fleet_steer_events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { SIGINT } from "../src/constants/signals.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
import {
  EVENT_ID,
  FLEET_ID,
  eventStream,
  makeLayer,
  makeRecorder,
  nullOutput,
  postedEvent,
  streamFrom,
} from "./helpers-fleet-steer.ts";

type SteerFrame = Parameters<typeof eventStream>[0][number];

const chunkFrame = (text: string): SteerFrame => ({
  id: null,
  type: KIND_CHUNK,
  data: { event_id: EVENT_ID, text },
});
const completeFrame = (): SteerFrame => ({
  id: null,
  type: KIND_EVENT_COMPLETE,
  data: { event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED },
});

describe("steer — abort inside the pre-id window", () => {
  test("a SIGINT before the POST returns discards buffered frames", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    const streamSignals: AbortSignal[] = [];
    const capturingStream: typeof import("../src/lib/sse.ts").streamGet = (url, headers, cb, options) => {
      if (options?.signal) streamSignals.push(options.signal);
      return eventStream([chunkFrame("early words"), completeFrame()])(url, headers, cb);
    };
    const httpReply = <T>(_input: HttpRequestInput): T => {
      signalSource.emit(SIGINT);
      return postedEvent<T>();
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["hi\n"], false),
        stdout: nullOutput(),
        streamGet: capturingStream,
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("early words"))).toBe(false);
    expect(streamSignals).toHaveLength(1);
    expect(streamSignals[0]?.aborted).toBe(true);
  });

  test("a SIGINT during the tail handshake suppresses the POST entirely", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    const abortAtOpen: typeof import("../src/lib/sse.ts").streamGet = async (_url, _headers, _cb, options) => {
      signalSource.emit(SIGINT);
      options?.onOpen?.();
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, undefined, { forceTty: true }, {
        stdin: streamFrom(["hi\n"], false),
        stdout: nullOutput(),
        streamGet: abortAtOpen,
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.requests).toHaveLength(0);
  });
});
