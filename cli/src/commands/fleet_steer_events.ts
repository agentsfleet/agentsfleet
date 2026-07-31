import { Effect, Redacted } from "effect";
import { EVENT_STATUS } from "../constants/event-status.ts";
import { authHeaders } from "../lib/http.ts";
import {
  streamGet as defaultStreamGet,
  type SseFrame,
  type StreamGetCallback,
} from "../lib/sse.ts";
import { ui } from "../output/index.ts";
import { CliConfig } from "../services/config.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import {
  wsFleetEventsPath,
  wsFleetEventsStreamPath,
} from "../lib/api-paths.ts";

const MS_FIELD = "ms";

const SSE_FALLBACK_TIMEOUT_MS = 60_000;
const FALLBACK_POLL_MS = 1_500;
const FALLBACK_POLL_LIMIT = 200;
const TOOL_PREFIX_LABEL = "[tool]" as const;
export const STATUS_COMPLETE = "complete" as const;
const FIELD_NAME = "name" as const;
const TYPE_OBJECT = "object" as const;
export const STATUS_SSE_DISCONNECTED = "sse_disconnected" as const;
export const STATUS_SSE_ERROR = "sse_error" as const;
const FIELD_STATUS = "status" as const;
const TYPE_STRING = "string" as const;
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

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === TYPE_OBJECT;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export type SteerOutcome =
  | { readonly kind: typeof STATUS_COMPLETE; readonly status: string }
  | { readonly kind: typeof STATUS_TIMEOUT }
  | { readonly kind: typeof STATUS_SSE_DISCONNECTED }
  | { readonly kind: typeof STATUS_SSE_ERROR; readonly detail: string };
export type PolledSteerOutcome =
  | Extract<SteerOutcome, { readonly kind: typeof STATUS_COMPLETE }>
  | Extract<SteerOutcome, { readonly kind: typeof STATUS_TIMEOUT }>;

interface EventsResponse {
  readonly items?: ReadonlyArray<EventRow>;
}

interface EventRow {
  readonly event_id?: string;
  readonly status?: string;
}

type StreamGetFn = typeof defaultStreamGet;

const isTerminal = (status: string): boolean =>
  status === EVENT_STATUS.PROCESSED ||
  status === EVENT_STATUS.FLEET_ERROR ||
  status === EVENT_STATUS.GATE_BLOCKED;

const eventIdToSince = (eventId: string): string | null => {
  const dash = eventId.indexOf("-");
  if (dash <= 0) return null;
  const ms = Number.parseInt(eventId.slice(0, dash), 10);
  if (!Number.isFinite(ms)) return null;
  const floored = ms - (ms % MS_PER_SECOND);
  return new Date(floored).toISOString().replace(/\.\d{3}Z$/, "Z");
};

interface SteerFrameHandlers {
  readonly printLine: (line: string) => void;
  readonly eventId: string;
}

const makeFrameCallback = (
  handlers: SteerFrameHandlers,
  setOutcome: (next: SteerOutcome) => void,
): StreamGetCallback => (event) => {
  const payload = event.data as Record<string, unknown> | null | undefined;
  if (!isRecord(payload)) return undefined;
  // Frames without an event_id are dropped too: the daemon stamps every
  // activity frame (activity_publisher), so an id-less frame's ownership is
  // unknown and rendering it would break the one-event-only invariant.
  const frameEventId = payload["event_id"];
  if (frameEventId !== handlers.eventId) return undefined;
  if (event.type === KIND_CHUNK && isString(payload[FIELD_TEXT])) {
    handlers.printLine(`${ui.dim("[claw]")} ${payload[FIELD_TEXT]}`);
    return undefined;
  }
  if (event.type === KIND_TOOL_CALL_STARTED && isString(payload[FIELD_NAME])) {
    handlers.printLine(`${ui.dim(TOOL_PREFIX_LABEL)} ${payload[FIELD_NAME]} starting`);
    return undefined;
  }
  if (event.type === KIND_TOOL_CALL_COMPLETED && isString(payload[FIELD_NAME])) {
    const ms = typeof payload[MS_FIELD] === "number" ? `${payload[MS_FIELD] as number}ms` : "";
    handlers.printLine(`${ui.dim(TOOL_PREFIX_LABEL)} ${payload[FIELD_NAME]} done ${ms}`);
    return undefined;
  }
  if (event.type === KIND_EVENT_COMPLETE) {
    const status = isString(payload[FIELD_STATUS]) ? payload[FIELD_STATUS] : "unknown";
    setOutcome({ kind: STATUS_COMPLETE, status });
    return false;
  }
  return undefined;
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

const linkedAbort = (signal?: AbortSignal): AbortController => {
  const ctrl = new AbortController();
  if (signal) {
    if (signal.aborted) ctrl.abort();
    else signal.addEventListener("abort", () => ctrl.abort(), { once: true });
  }
  return ctrl;
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
      Effect.runSync(output.info(line));
    };
    let outcome: SteerOutcome = { kind: STATUS_SSE_DISCONNECTED };
    const ctrl = linkedAbort(signal);
    const buffer = makePreIdBuffer();
    let markOpened: () => void = () => {};
    const opened = new Promise<void>((resolve) => {
      markOpened = resolve;
    });
    const finished = streamGet(url, headers, buffer.cb, { signal: ctrl.signal, onOpen: markOpened })
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
      const filtered = makeFrameCallback({ printLine, eventId: id }, (next) => {
        outcome = next;
      });
      if (!buffer.promote(filtered)) ctrl.abort();
    };
    return Object.freeze({
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
): Effect.Effect<PolledSteerOutcome, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const sinceParam = eventIdToSince(eventId);
    const deadline = Date.now() + SSE_FALLBACK_TIMEOUT_MS;
    while (Date.now() < deadline && !signal?.aborted) { // oxlint-disable-line no-unmodified-loop-condition -- clock + external AbortSignal terminate it
      const path = `${wsFleetEventsPath(wsId, fleetId)}?limit=${FALLBACK_POLL_LIMIT}${sinceParam ? `&since=${encodeURIComponent(sinceParam)}` : ""}`;
      const res = yield* http.request<EventsResponse>({ path, token }).pipe(
        Effect.orElseSucceed((): EventsResponse => ({ items: [] })),
      );
      const match = (res.items ?? []).find((row: EventRow) => row.event_id === eventId);
      if (match && isString(match.status) && isTerminal(match.status)) {
        return { kind: STATUS_COMPLETE, status: match.status } as PolledSteerOutcome;
      }
      if (signal?.aborted) break;
      yield* Effect.sleep(`${FALLBACK_POLL_MS} millis`);
    }
    return { kind: STATUS_TIMEOUT } as PolledSteerOutcome;
  });
