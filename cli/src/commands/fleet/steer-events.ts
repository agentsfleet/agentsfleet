import { ActivityPrinter, activityText } from "./task-activity.ts";
import { ToolActivityPrinter } from "./tool-activity.ts";
import { setTimeout as delay } from "node:timers/promises";
import { Effect, Option, Redacted } from "effect";
import { CLI_ERROR_TAG, type CliError } from "../../errors/index.ts";
import { EVENT_STATUS } from "../../constants/event-status.ts";
import { authHeaders } from "../../lib/http.ts";
import {
  streamGet as defaultStreamGet,
  type SseFrame,
  type StreamGetCallback,
} from "../../lib/sse.ts";
import { ui } from "../../output/index.ts";
import { CliConfig } from "../../services/config.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import {
  wsFleetEventPath,
  wsFleetEventsStreamPath,
} from "../../lib/api-paths.ts";
import { isString, isRecord } from "../../lib/guards.ts";

const SSE_FALLBACK_TIMEOUT_MS = 60_000;
const FALLBACK_POLL_MS = 1_500;
const EVENT_NOT_FOUND_CODE = "UZ-AGT-015";
export const STATUS_COMPLETE = "complete" as const;
export const STATUS_SSE_DISCONNECTED = "sse_disconnected" as const;
export const STATUS_SSE_ERROR = "sse_error" as const;
const FIELD_STATUS = "status" as const;
const FIELD_TEXT = "text" as const;
export const STATUS_TIMEOUT = "timeout" as const;
const MS_PER_SECOND = 1000 as const;

// Frame kinds mirror the daemon's activity_publisher KIND_* constants — the
// wire values these frames arrive with.
export const KIND_CHUNK = "chunk" as const;
export const KIND_TOOL_CALL_STARTED = "tool_call_started" as const;
export const KIND_TOOL_CALL_COMPLETED = "tool_call_completed" as const;
export const KIND_EVENT_COMPLETE = "event_complete" as const;

const BYTES_PER_KIB = 1024;
// Pre-id buffer caps, sized to one POST round-trip of frames: until the 202
// names the event nothing can be filtered or rendered, so frames wait here.
// Overflow drops oldest — the durable events poll stays the recovery backstop.
export const PRE_ID_BUFFER_MAX_FRAMES = 256;
export const PRE_ID_BUFFER_MAX_BYTES = 256 * BYTES_PER_KIB;
// Longest the POST will wait for the tail's subscription to be live. A tail
// that can't open inside this window degrades to post-then-poll; the send is
// never blocked on a broken stream.
export const TAIL_OPEN_MAX_WAIT_MS = 2_000;

export const SSE_FALLBACK_TIMEOUT_SECONDS = Math.round(
  SSE_FALLBACK_TIMEOUT_MS / MS_PER_SECOND,
);

export type SteerOutcome =
  | { readonly kind: typeof STATUS_COMPLETE; readonly status: string }
  | { readonly kind: typeof STATUS_TIMEOUT }
  | { readonly kind: typeof STATUS_SSE_DISCONNECTED }
  | { readonly kind: typeof STATUS_SSE_ERROR; readonly detail: string };
export type PolledSteerOutcome =
  | Extract<SteerOutcome, { readonly kind: typeof STATUS_COMPLETE }>
  | Extract<SteerOutcome, { readonly kind: typeof STATUS_TIMEOUT }>;

type StreamGetFn = typeof defaultStreamGet;

const isTerminal = (status: string): boolean =>
  status === EVENT_STATUS.PROCESSED ||
  status === EVENT_STATUS.FLEET_ERROR ||
  status === EVENT_STATUS.GATE_BLOCKED;

interface SteerFrameHandlers {
  readonly printLine: (line: string) => void;
  readonly eventId: string;
  readonly presentation: SteerPresentation;
}

interface SteerPresentation {
  readonly activity: ActivityPrinter;
  readonly tools: ToolActivityPrinter;
}

const makeFrameCallback = (
  handlers: SteerFrameHandlers,
  setOutcome: (next: SteerOutcome) => void,
): StreamGetCallback => {
  const {activity, tools} = handlers.presentation;
  return (event) => {
  const payload = event.data as Record<string, unknown> | null | undefined;
  if (!isRecord(payload)) return undefined;
  // Frames without an event_id are dropped too: the daemon stamps every
  // activity frame (activity_publisher), so an id-less frame's ownership is
  // unknown and rendering it would break the one-event-only invariant.
  const frameEventId = payload["event_id"];
  if (frameEventId !== handlers.eventId) return undefined;
  if (activity.frame(event.type, payload)) return undefined;
  if (tools.frame(event.type, payload)) return undefined;
  if (event.type === KIND_CHUNK && payload["text_kind"] === "reasoning") return undefined;
  if (event.type === KIND_CHUNK && isString(payload[FIELD_TEXT])) {
    handlers.printLine(`${ui.dim("[claw]")} ${activityText(payload[FIELD_TEXT])}`);
    return undefined;
  }
  if (event.type === KIND_EVENT_COMPLETE) {
    if (isString(payload["final_reply"]) && payload["final_reply"]) handlers.printLine(`Final reply:\n${activityText(payload["final_reply"])}`);
    const status = isString(payload[FIELD_STATUS]) ? payload[FIELD_STATUS] : "unknown";
    setOutcome({ kind: STATUS_COMPLETE, status });
    return false;
  }
  return undefined;
  };
};

// How the pre-send wait ended. A timed-out tail is untrusted for the turn:
// it may have missed the opening frames, so rendering from it could present
// a truncated reply as complete — the caller closes it and polls instead.
export const TAIL_OPENED = "tail_opened" as const;
export const TAIL_SETTLED = "tail_settled" as const;
export const TAIL_TIMED_OUT = "tail_timed_out" as const;
export type TailReadiness =
  | typeof TAIL_OPENED
  | typeof TAIL_SETTLED
  | typeof TAIL_TIMED_OUT;

export interface EventTailHandle {
  readonly presentation: SteerPresentation;
  readonly awaitReady: () => Promise<TailReadiness>;
  readonly awaitOutcome: () => Promise<SteerOutcome>;
  readonly deliverEventId: (id: string) => void;
  readonly close: () => Promise<void>;
}

interface BufferedFrame {
  readonly frame: SseFrame;
  readonly bytes: number;
}

interface PreIdBuffer {
  readonly cb: StreamGetCallback;
  readonly promote: (filtered: StreamGetCallback) => boolean;
  readonly droppedCount: () => number;
}

// Buffers every frame until the event id is known, then hands the stream to a
// filtered callback: buffered frames replay through it in arrival order, and
// `promote` returns false when a replayed frame asks the stream to stop.
const makePreIdBuffer = (): PreIdBuffer => {
  const entries: BufferedFrame[] = [];
  let totalBytes = 0;
  let droppedFrames = 0;
  let live: StreamGetCallback | null = null;
  const cb: StreamGetCallback = (event) => {
    if (live) return live(event);
    // Buffer.byteLength counts encoded bytes (not UTF-16 code units), keeping
    // the cap honest for multi-byte chunk text.
    const bytes = Buffer.byteLength(JSON.stringify(event));
    entries.push({ frame: event, bytes });
    totalBytes += bytes;
    while (
      entries.length > PRE_ID_BUFFER_MAX_FRAMES ||
      totalBytes > PRE_ID_BUFFER_MAX_BYTES
    ) {
      const dropped = entries.shift();
      if (!dropped) break;
      totalBytes -= dropped.bytes;
      droppedFrames += 1;
    }
    return undefined;
  };
  const promote = (filtered: StreamGetCallback): boolean => {
    live = filtered;
    for (const entry of entries.splice(0)) {
      if (filtered(entry.frame) === false) return false;
    }
    return true;
  };
  return Object.freeze({ cb, promote, droppedCount: () => droppedFrames });
};

// Ready = headers accepted (subscription live) OR the stream settled (a
// broken tail must not block the send) OR the bounded wait elapsed. The
// losing timer is always cleared; the caller branches on which one won.
const boundedReady = (
  opened: Promise<void>,
  finished: Promise<unknown>,
): (() => Promise<TailReadiness>) => () => {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const bounded = new Promise<TailReadiness>((resolve) => {
    timer = setTimeout(() => resolve(TAIL_TIMED_OUT), TAIL_OPEN_MAX_WAIT_MS);
  });
  return Promise.race([
    opened.then((): TailReadiness => TAIL_OPENED),
    finished.then((): TailReadiness => TAIL_SETTLED),
    bounded,
  ]).finally(() => {
    if (timer) clearTimeout(timer);
  });
};

// Opens the live tail BEFORE the message posts, so no frame of the steered
// event can be published before a subscriber exists (the activity channel has
// no replay). `awaitOutcome` is a thunk: it reads the outcome only after the
// stream settles AND any pending replay ran, so a stream that ended pre-id
// still reports a buffered event_complete instead of a stale disconnect.
export const openEventTail = (
  wsId: string,
  fleetId: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
  signal?: AbortSignal,
): Effect.Effect<EventTailHandle, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const url = `${config.apiUrl.replace(/\/$/, "")}${wsFleetEventsStreamPath(wsId, fleetId)}`;
    const headers = authHeaders({ token: Redacted.value(token) });
    const printLine = (line: string): void => {
      if (output.format === OUTPUT_FORMAT.text) Effect.runSync(output.info(line));
    };
    const presentation = {activity: new ActivityPrinter(printLine), tools: new ToolActivityPrinter(printLine)};
    let outcome: SteerOutcome = { kind: STATUS_SSE_DISCONNECTED };
    const ctrl = new AbortController();
    const streamSignal = signal ? AbortSignal.any([signal, ctrl.signal]) : ctrl.signal;
    const buffer = makePreIdBuffer();
    const { promise: opened, resolve: markOpened } = Promise.withResolvers<void>();
    const finished = streamGet(url, headers, buffer.cb, { signal: streamSignal, onOpen: markOpened })
      .then((): SteerOutcome | null => null)
      .catch(
        (err): SteerOutcome => ({
          kind: STATUS_SSE_ERROR,
          detail: err instanceof Error ? err.message : String(err),
        }),
      );
    const deliverEventId = (id: string): void => {
      // Overflow must never be silent: a truncated replay with a retained
      // event_complete would otherwise pass itself off as the whole reply.
      const dropped = buffer.droppedCount();
      if (dropped > 0) {
        printLine(ui.dim(`${dropped} early frame(s) dropped live — full history: agentsfleet events ${fleetId}`));
      }
      const filtered = makeFrameCallback({ printLine, eventId: id, presentation }, (next) => {
        outcome = next;
      });
      if (!buffer.promote(filtered)) ctrl.abort();
    };
    return Object.freeze({
      presentation,
      awaitReady: boundedReady(opened, finished),
      // A buffered event_complete wins over a late stream error — the answer
      // is already known, so no redundant poll.
      awaitOutcome: (): Promise<SteerOutcome> =>
        finished.then((sseError) =>
          outcome.kind === STATUS_COMPLETE ? outcome : sseError ?? outcome,
        ),
      deliverEventId,
      // Joins the stream's settlement so a failed turn never returns while
      // its client-side teardown is still in flight.
      close: (): Promise<void> => {
        ctrl.abort();
        return finished.then((): void => undefined);
      },
    });
  });

export const pollEventTerminal = (
  wsId: string,
  fleetId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
  presentation?: SteerPresentation,
): Effect.Effect<PolledSteerOutcome, CliError, HttpClient | Output> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const output = yield* Output;
    const printLine = (line: string): void => {
      if (output.format === OUTPUT_FORMAT.text) Effect.runSync(output.info(line));
    };
    const activity = presentation?.activity ?? new ActivityPrinter(printLine);
    const tools = presentation?.tools ?? new ToolActivityPrinter(printLine);
    printLine("Reading saved activity after the live stream ended.");
    // Event IDs are opaque. The owned detail read avoids inferring a clock
    // from an ID or losing this event behind a busy fleet's newest page.
    const path = wsFleetEventPath(wsId, fleetId, eventId);
    const deadline = Date.now() + SSE_FALLBACK_TIMEOUT_MS;
    let observed = false;
    while (Date.now() < deadline && !signal?.aborted) { // oxlint-disable-line no-unmodified-loop-condition -- clock + external AbortSignal terminate it
      const remaining = deadline - Date.now();
      const read = yield* http.request<unknown>({ path, token, signal, timeoutMs: remaining, retry: { maxAttempts: 1 } }).pipe(
        // Admission can return before the exact event reaches the read store.
        Effect.catch((error) => signal?.aborted || error.transient || (!observed &&
          error._tag === CLI_ERROR_TAG.server && error.status === 404 && error.code === EVENT_NOT_FOUND_CODE)
          ? Effect.succeed(null) : Effect.fail(error)),
        Effect.timeoutOption(`${remaining} millis`),
      );
      if (Option.isNone(read)) return { kind: STATUS_TIMEOUT } as PolledSteerOutcome;
      const res = read.value;
      if (isRecord(res) && res.event_id === eventId) activity.history(res["task_activity"]);
      if (isRecord(res) && res.event_id === eventId && isString(res.status) && isTerminal(res.status)) {
        tools.history(res["tool_calls"]);
        if (isString(res["response_text"]) && res["response_text"]) printLine(`Final reply:\n${activityText(res["response_text"])}`);
        return { kind: STATUS_COMPLETE, status: res.status } as PolledSteerOutcome;
      }
      if (isRecord(res) && res.event_id === eventId) observed = true;
      if (signal?.aborted) break;
      const pause = Math.min(FALLBACK_POLL_MS, deadline - Date.now());
      if (pause <= 0) break;
      yield* Effect.tryPromise((owner) =>
        delay(pause, undefined, { signal: signal ? AbortSignal.any([signal, owner]) : owner }),
      ).pipe(Effect.ignore);
    }
    return { kind: STATUS_TIMEOUT } as PolledSteerOutcome;
  });
