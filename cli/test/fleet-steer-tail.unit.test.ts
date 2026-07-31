// Unit coverage for the steer event tail (openEventTail): the pre-id buffer
// (replay order, foreign/id-less drop, frame + byte bounds), the post-promote
// live path, tail lifecycle on success / late error / abort, and the
// subscription-ready gate. Split from fleet-steer-linecov.unit.test.ts when
// that file crossed the size cap.

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Redacted } from "effect";

import { steerEffectFromArgs } from "../src/commands/fleet_steer.ts";
import {
  KIND_CHUNK,
  KIND_EVENT_COMPLETE,
  PRE_ID_BUFFER_MAX_BYTES,
  PRE_ID_BUFFER_MAX_FRAMES,
  STATUS_COMPLETE,
  STATUS_SSE_DISCONNECTED,
  openEventTail,
} from "../src/commands/fleet_steer_events.ts";
import { EVENT_STATUS } from "../src/constants/event-status.ts";
import { SIGINT } from "../src/constants/signals.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { ReplSignalEmitter } from "../src/lib/repl.ts";
import {
  EVENT_ID,
  FLEET_ID,
  OTHER_EVENT_ID,
  POST,
  SINGLE_MESSAGE,
  TOKEN,
  WS_ID,
  eventStream,
  makeLayer,
  makeRecorder,
  nullOutput,
  postedEvent,
  streamFrom,
} from "./fleet-steer.integration.test.ts";

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
    // No overflow → no truncation notice.
    expect(rec.stdout.some((l) => l.includes("dropped live"))).toBe(false);
  });

  test("test_foreign_event_frames_dropped", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: eventStream([
          chunkFrame(OTHER_EVENT_ID, "foreign words"),
          // An id-less frame's ownership is unknown — dropped, never rendered.
          { id: null, type: KIND_CHUNK, data: { text: "ghost words" } },
          chunkFrame(EVENT_ID, "our words"),
          completeFrame(EVENT_ID),
        ]),
      }).pipe(Effect.provide(makeLayer(rec))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("foreign words"))).toBe(false);
    expect(rec.stdout.some((l) => l.includes("ghost words"))).toBe(false);
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
    // Truncation is never silent: the drop notice names the recovery command.
    expect(rec.stdout.some((l) => l.includes("dropped live"))).toBe(true);

    // Byte bound: two half-cap frames exceed the byte cap; oldest drops.
    // Multi-byte char pins byte semantics — each 💠 is 4 UTF-8 bytes but only
    // 2 UTF-16 code units, so code-unit counting would never overflow here.
    const rec2 = makeRecorder();
    const half = "💠".repeat(PRE_ID_BUFFER_MAX_BYTES / 8);
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
    expect(rec2.stdout.some((l) => l.includes("dropped live"))).toBe(true);
  });
});

// ── Post-promote live path: frames after the id bypass the buffer ─────────

describe("steer — frames after the event is named render live", () => {
  test("post-promote frames render immediately and event_complete stops the stream", async () => {
    const rec = makeRecorder();
    let pump: ((frame: SteerFrame) => boolean | void) | undefined;
    let holdOpen: () => void = () => {};
    const manualStream: typeof import("../src/lib/sse.ts").streamGet = (_url, _headers, cb) =>
      new Promise<void>((resolveStream) => {
        pump = cb;
        holdOpen = resolveStream;
      });
    const handle = await Effect.runPromise(
      openEventTail(WS_ID, FLEET_ID, Redacted.make(TOKEN), manualStream).pipe(
        Effect.provide(makeLayer(rec)),
      ),
    );
    handle.deliverEventId(EVENT_ID);
    pump?.(chunkFrame(EVENT_ID, "live words"));
    // Rendered synchronously through the promoted filter — never buffered.
    expect(rec.stdout.some((l) => l.includes("live words"))).toBe(true);
    // An id-less event_complete must not terminate the tail — its ownership
    // is unknown, so it is dropped (undefined), not forwarded as false.
    expect(pump?.({ id: null, type: KIND_EVENT_COMPLETE, data: { status: EVENT_STATUS.PROCESSED } })).toBeUndefined();
    // The stream-stopping false must be forwarded through the live path;
    // dropping it would hang every steer until the 60s SSE timeout.
    expect(pump?.(completeFrame(EVENT_ID))).toBe(false);
    holdOpen();
    const outcome = await handle.awaitOutcome();
    expect(outcome.kind).toBe(STATUS_COMPLETE);
  });

  test("a pre-id frame replays before post-promote frames render live", async () => {
    const rec = makeRecorder();
    let pump: ((frame: SteerFrame) => boolean | void) | undefined;
    let holdOpen: () => void = () => {};
    const manualStream: typeof import("../src/lib/sse.ts").streamGet = (_url, _headers, cb) =>
      new Promise<void>((resolveStream) => {
        pump = cb;
        holdOpen = resolveStream;
      });
    const handle = await Effect.runPromise(
      openEventTail(WS_ID, FLEET_ID, Redacted.make(TOKEN), manualStream).pipe(
        Effect.provide(makeLayer(rec)),
      ),
    );
    pump?.(chunkFrame(EVENT_ID, "first words"));
    expect(rec.stdout).toEqual([]);
    handle.deliverEventId(EVENT_ID);
    pump?.(chunkFrame(EVENT_ID, "second words"));
    const first = rec.stdout.findIndex((l) => l.includes("first words"));
    const second = rec.stdout.findIndex((l) => l.includes("second words"));
    expect(first).toBeGreaterThanOrEqual(0);
    expect(second).toBeGreaterThan(first);
    holdOpen();
    handle.close();
    const outcome = await handle.awaitOutcome();
    expect(outcome.kind).toBe(STATUS_SSE_DISCONNECTED);
  });
});

// ── Success path releases the tail; buffered answers beat late errors ─────

describe("steer — tail lifecycle on success and late stream error", () => {
  test("after a successful steer the acquire/release scope aborts the tail", async () => {
    const rec = makeRecorder();
    const streamSignals: AbortSignal[] = [];
    // Silent stream: no frames, clean end — only the release scope can abort
    // this signal, so the assertion isolates close-on-success.
    const silentCapturing: typeof import("../src/lib/sse.ts").streamGet = (_url, _headers, _cb, options) => {
      if (options?.signal) streamSignals.push(options.signal);
      options?.onOpen?.();
      return Promise.resolve();
    };
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return postedEvent<T>();
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: silentCapturing,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(streamSignals).toHaveLength(1);
    expect(streamSignals[0]?.aborted).toBe(true);
  });

  test("a buffered event_complete wins over a late stream error — no redundant poll", async () => {
    const rec = makeRecorder();
    let pump: ((frame: SteerFrame) => boolean | void) | undefined;
    let failStream: (err: Error) => void = () => {};
    const failingAfterFrames: typeof import("../src/lib/sse.ts").streamGet = (_url, _headers, cb) =>
      new Promise<void>((_resolve, reject) => {
        pump = cb;
        failStream = reject;
      });
    const handle = await Effect.runPromise(
      openEventTail(WS_ID, FLEET_ID, Redacted.make(TOKEN), failingAfterFrames).pipe(
        Effect.provide(makeLayer(rec)),
      ),
    );
    pump?.(completeFrame(EVENT_ID));
    failStream(new Error("stream torn down"));
    handle.deliverEventId(EVENT_ID);
    const outcome = await handle.awaitOutcome();
    expect(outcome.kind).toBe(STATUS_COMPLETE);
  });
});

// ── Ready-timeout: a tail that never opens is closed unheard ──────────────

describe("steer — a tail that never becomes ready is closed and the poll decides", () => {
  test("ready-timeout closes the tail and resolves the turn from the durable poll", async () => {
    const rec = makeRecorder();
    const streamSignals: AbortSignal[] = [];
    // Never calls onOpen and never settles until aborted: the only way this
    // turn can succeed (before the test timeout) is the timed-out branch —
    // the live path would wait forever on a stream that never ends.
    const neverReady: typeof import("../src/lib/sse.ts").streamGet = (_url, _headers, _cb, options) =>
      new Promise<void>((resolveStream) => {
        if (options?.signal) streamSignals.push(options.signal);
        options?.signal?.addEventListener("abort", () => resolveStream(), { once: true });
      });
    const httpReply = <T>(input: HttpRequestInput): T => {
      if (input.method === POST) return postedEvent<T>();
      return { items: [{ event_id: EVENT_ID, status: EVENT_STATUS.PROCESSED }] } as T;
    };
    const exit = await Effect.runPromiseExit(
      steerEffectFromArgs(FLEET_ID, SINGLE_MESSAGE, {}, {
        stdin: streamFrom([], false),
        stdout: nullOutput(),
        streamGet: neverReady,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.requests.some((r) => r.method !== POST)).toBe(true);
    expect(rec.stdout.join("\n")).toContain(`${EVENT_ID} ${EVENT_STATUS.PROCESSED}`);
    expect(streamSignals[0]?.aborted).toBe(true);
  }, 10_000);
});

// ── Already-aborted signal at tail open ───────────────────────────────────

describe("steer — tail opened with an already-aborted signal", () => {
  test("should deliver the aborted signal to the stream and render nothing", async () => {
    const rec = makeRecorder();
    const ctrl = new AbortController();
    ctrl.abort();
    let sawAbortedSignal = false;
    const signalProbe: typeof import("../src/lib/sse.ts").streamGet = async (
      _url,
      _headers,
      _cb,
      options,
    ): Promise<void> => {
      sawAbortedSignal = options?.signal?.aborted === true;
    };
    const handle = await Effect.runPromise(
      openEventTail(WS_ID, FLEET_ID, Redacted.make(TOKEN), signalProbe, ctrl.signal).pipe(
        Effect.provide(makeLayer(rec)),
      ),
    );
    const outcome = await handle.awaitOutcome();
    expect(sawAbortedSignal).toBe(true);
    expect(outcome.kind).toBe(STATUS_SSE_DISCONNECTED);
    expect(rec.stdout).toEqual([]);
  });
});

// ── Abort inside the pre-id window ────────────────────────────────────────

describe("steer — abort inside the pre-id window", () => {
  test("test_abort_in_pre_id_window", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    const streamSignals: AbortSignal[] = [];
    const capturingStream: typeof import("../src/lib/sse.ts").streamGet = (url, headers, cb, options) => {
      if (options?.signal) streamSignals.push(options.signal);
      return eventStream([
        chunkFrame(EVENT_ID, "early words"),
        completeFrame(EVENT_ID),
      ])(url, headers, cb);
    };
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
        streamGet: capturingStream,
        signalSource,
      }).pipe(Effect.provide(makeLayer(rec, httpReply))),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("early words"))).toBe(false);
    // The interrupted turn closed its tail — the stream's signal is aborted.
    expect(streamSignals).toHaveLength(1);
    expect(streamSignals[0]?.aborted).toBe(true);
  });

  test("a SIGINT during the tail handshake suppresses the POST entirely", async () => {
    const rec = makeRecorder();
    const signalSource = new ReplSignalEmitter();
    const abortAtOpen: typeof import("../src/lib/sse.ts").streamGet = async (_url, _headers, _cb, options) => {
      // The cancel lands before the subscription is ready, so the turn must
      // interrupt without ever sending the message.
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
