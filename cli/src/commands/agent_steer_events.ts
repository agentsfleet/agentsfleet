import { Effect, Redacted } from "effect";
import { EVENT_STATUS } from "../constants/event-status.ts";
import { authHeaders } from "../lib/http.ts";
import { streamGet as defaultStreamGet, type StreamGetCallback } from "../lib/sse.ts";
import { ui } from "../output/index.ts";
import { CliConfig } from "../services/config.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import {
  wsAgentEventsPath,
  wsAgentEventsStreamPath,
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
  status === EVENT_STATUS.AGENT_ERROR ||
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
  const frameEventId = payload["event_id"];
  if (frameEventId && frameEventId !== handlers.eventId) return undefined;
  if (event.type === "chunk" && isString(payload[FIELD_TEXT])) {
    handlers.printLine(`${ui.dim("[claw]")} ${payload[FIELD_TEXT]}`);
    return undefined;
  }
  if (event.type === "tool_call_started" && isString(payload[FIELD_NAME])) {
    handlers.printLine(`${ui.dim(TOOL_PREFIX_LABEL)} ${payload[FIELD_NAME]} starting`);
    return undefined;
  }
  if (event.type === "tool_call_completed" && isString(payload[FIELD_NAME])) {
    const ms = typeof payload[MS_FIELD] === "number" ? `${payload[MS_FIELD] as number}ms` : "";
    handlers.printLine(`${ui.dim(TOOL_PREFIX_LABEL)} ${payload[FIELD_NAME]} done ${ms}`);
    return undefined;
  }
  if (event.type === "event_complete") {
    const status = isString(payload[FIELD_STATUS]) ? payload[FIELD_STATUS] : "unknown";
    setOutcome({ kind: STATUS_COMPLETE, status });
    return false;
  }
  return undefined;
};

export const tailEventStream = (
  wsId: string,
  agentId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
  signal?: AbortSignal,
): Effect.Effect<SteerOutcome, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const url = `${config.apiUrl.replace(/\/$/, "")}${wsAgentEventsStreamPath(wsId, agentId)}`;
    const headers = authHeaders({ token: Redacted.value(token) });
    const printLine = (line: string): void => {
      Effect.runSync(output.info(line));
    };
    let outcome: SteerOutcome = { kind: STATUS_SSE_DISCONNECTED };
    const cb = makeFrameCallback({ printLine, eventId }, (next) => {
      outcome = next;
    });

    const work = Effect.tryPromise<void, SteerOutcome>({
      try: async () => {
        await streamGet(url, headers, cb, signal ? { signal } : undefined);
      },
      catch: (err): SteerOutcome => ({
        kind: STATUS_SSE_ERROR,
        detail: err instanceof Error ? err.message : String(err),
      }),
    });
    return yield* work.pipe(
      Effect.match({
        onSuccess: (): SteerOutcome => outcome,
        onFailure: (sseError): SteerOutcome => sseError,
      }),
    );
  });

export const pollEventTerminal = (
  wsId: string,
  agentId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
): Effect.Effect<PolledSteerOutcome, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const sinceParam = eventIdToSince(eventId);
    const deadline = Date.now() + SSE_FALLBACK_TIMEOUT_MS;
    while (Date.now() < deadline && !signal?.aborted) { // oxlint-disable-line no-unmodified-loop-condition -- clock + external AbortSignal terminate it
      const path = `${wsAgentEventsPath(wsId, agentId)}?limit=${FALLBACK_POLL_LIMIT}${sinceParam ? `&since=${encodeURIComponent(sinceParam)}` : ""}`;
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
