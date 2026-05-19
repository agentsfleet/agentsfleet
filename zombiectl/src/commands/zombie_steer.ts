// `zombiectl steer <zombie_id> "<message>"` — batch steer + stream.
//
//   1. POST  /messages          → captures `event_id` from the 202.
//   2. Opens GET /events/stream (SSE) with the bearer.
//   3. For every frame matching `event_id`, prints `[claw] <chunk>`
//      / `[tool] ...`; stops on `event_complete`.
//   4. If SSE drops mid-event, falls back to polling
//      GET /events?since=<event_id_ms>&limit=1 until the row reaches
//      a terminal status (60 s timeout).
//
// Interactive REPL (no message) lands in a follow-up. Batch mode is
// the primary integration with chat-style UIs and operator scripts.

import { Effect, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import {
  wsZombieMessagesPath,
  wsZombieEventsPath,
  wsZombieEventsStreamPath,
} from "../lib/api-paths.ts";
import { streamGet as defaultStreamGet, type StreamGetCallback } from "../lib/sse.ts";
import { EVENT_STATUS } from "../constants/event-status.ts";
import { ui } from "../output/index.ts";
import { authHeaders } from "../lib/http.ts";
import {
  ConfigError,
  ServerError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const SSE_FALLBACK_TIMEOUT_MS = 60_000;
const FALLBACK_POLL_MS = 1_500;
const FALLBACK_POLL_LIMIT = 200;

type SteerOutcome =
  | { readonly kind: "complete"; readonly status: string }
  | { readonly kind: "timeout" }
  | { readonly kind: "sse_disconnected" }
  | { readonly kind: "sse_error"; readonly detail: string };

interface MessagesResponse {
  readonly event_id?: string;
}

interface EventsResponse {
  readonly items?: ReadonlyArray<EventRow>;
}

interface EventRow {
  readonly event_id?: string;
  readonly status?: string;
}

type StreamGetFn = typeof defaultStreamGet;

export interface SteerDeps {
  readonly streamGet?: StreamGetFn;
}

const isTerminal = (status: string): boolean =>
  status === EVENT_STATUS.PROCESSED ||
  status === EVENT_STATUS.AGENT_ERROR ||
  status === EVENT_STATUS.GATE_BLOCKED;

// Redis stream IDs are `<ms>-<seq>`. The events endpoint's `since=`
// accepts RFC 3339 (`YYYY-MM-DDTHH:MM:SSZ`, no fractional seconds).
// Convert the milliseconds prefix back, rounded to the start of the
// second the message was XADDed so the row itself is included.
const eventIdToSince = (eventId: string): string | null => {
  const dash = eventId.indexOf("-");
  if (dash <= 0) return null;
  const ms = Number.parseInt(eventId.slice(0, dash), 10);
  if (!Number.isFinite(ms)) return null;
  const floored = ms - (ms % 1000);
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
  if (!payload || typeof payload !== "object") return undefined;
  const frameEventId = payload["event_id"];
  if (frameEventId && frameEventId !== handlers.eventId) return undefined;
  if (event.type === "chunk" && typeof payload["text"] === "string") {
    handlers.printLine(`${ui.dim("[claw]")} ${payload["text"] as string}`);
    return undefined;
  }
  if (event.type === "tool_call_started" && typeof payload["name"] === "string") {
    handlers.printLine(`${ui.dim("[tool]")} ${payload["name"] as string} starting`);
    return undefined;
  }
  if (event.type === "tool_call_completed" && typeof payload["name"] === "string") {
    const ms = typeof payload["ms"] === "number" ? `${payload["ms"] as number}ms` : "";
    handlers.printLine(`${ui.dim("[tool]")} ${payload["name"] as string} done ${ms}`);
    return undefined;
  }
  if (event.type === "event_complete") {
    const status =
      typeof payload["status"] === "string" ? (payload["status"] as string) : "unknown";
    setOutcome({ kind: "complete", status });
    return false;
  }
  return undefined;
};

const tailEventStream = (
  wsId: string,
  zombieId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
): Effect.Effect<SteerOutcome, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const url = `${config.apiUrl.replace(/\/$/, "")}${wsZombieEventsStreamPath(wsId, zombieId)}`;
    const headers = authHeaders({ token: Redacted.value(token) });
    const printLine = (line: string): void => {
      Effect.runSync(output.info(line));
    };
    // Mutated by `event_complete` in the SSE callback below; read after
    // the stream resolves. Default = disconnected (stream closed before
    // an event_complete frame).
    let outcome: SteerOutcome = { kind: "sse_disconnected" };
    const cb = makeFrameCallback({ printLine, eventId }, (next) => {
      outcome = next;
    });

    const work = Effect.tryPromise<void, SteerOutcome>({
      try: async () => {
        await streamGet(url, headers, cb);
      },
      catch: (err): SteerOutcome => ({
        kind: "sse_error",
        detail: err instanceof Error ? err.message : String(err),
      }),
    });
    // streamGet runs to completion → success arm returns the mutated
    // `outcome` (set by event_complete inside the callback, or left as
    // sse_disconnected). Throw path lands in failure arm with a
    // pre-shaped SteerOutcome.
    return yield* work.pipe(
      Effect.match({
        onSuccess: (): SteerOutcome => outcome,
        onFailure: (sseError): SteerOutcome => sseError,
      }),
    );
  });

const pollEventTerminal = (
  wsId: string,
  zombieId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
): Effect.Effect<SteerOutcome, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const sinceParam = eventIdToSince(eventId);
    const deadline = Date.now() + SSE_FALLBACK_TIMEOUT_MS;
    while (Date.now() < deadline) {
      const path = `${wsZombieEventsPath(wsId, zombieId)}?limit=${FALLBACK_POLL_LIMIT}${sinceParam ? `&since=${encodeURIComponent(sinceParam)}` : ""}`;
      const res = yield* http.request<EventsResponse>({ path, token }).pipe(
        Effect.orElseSucceed((): EventsResponse => ({ items: [] })),
      );
      const match = (res.items ?? []).find((row: EventRow) => row.event_id === eventId);
      if (match && typeof match.status === "string" && isTerminal(match.status)) {
        return { kind: "complete", status: match.status } as SteerOutcome;
      }
      yield* Effect.sleep(`${FALLBACK_POLL_MS} millis`);
    }
    return { kind: "timeout" } as SteerOutcome;
  });

const renderOutcome = (
  outcome: SteerOutcome,
  eventId: string,
  zombieId: string,
): Effect.Effect<void, CliError, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;

    if (config.jsonMode) {
      yield* output.printJson({ event_id: eventId, ...outcome });
    } else if (outcome.kind === "complete") {
      yield* output.info("");
      yield* output.success(`event ${eventId} ${outcome.status}`);
    } else if (outcome.kind === "timeout") {
      yield* output.error(
        `event ${eventId} still in flight after ${Math.round(SSE_FALLBACK_TIMEOUT_MS / 1000)}s — check: zombiectl events ${zombieId}`,
      );
    } else if (outcome.kind === "sse_error") {
      yield* output.error(`message failed: ${outcome.kind} — ${outcome.detail}`);
    } else {
      yield* output.error(`message failed: ${outcome.kind}`);
    }

    if (outcome.kind === "complete") {
      if (outcome.status !== EVENT_STATUS.PROCESSED) {
        return yield* Effect.fail(
          new ConfigError({
            detail: `event ${eventId} terminated with status: ${outcome.status}`,
            suggestion: `inspect: zombiectl events ${zombieId}`,
          }),
        );
      }
      return;
    }
    return yield* Effect.fail(
      new ConfigError({
        detail: `event ${eventId} did not complete (${outcome.kind})`,
        suggestion: `retry, or inspect: zombiectl events ${zombieId}`,
      }),
    );
  });

export const steerEffectFromArgs = (
  zombieId: string | undefined,
  message: string | undefined,
  deps: SteerDeps = {},
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const streamGet = deps.streamGet ?? defaultStreamGet;

    if (!zombieId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "zombie_id is required",
          suggestion: 'usage: zombiectl steer <zombie_id> "<message>"',
        }),
      );
    }
    if (!message || message.trim().length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "interactive steer is not yet implemented",
          suggestion: 'pass a message: zombiectl steer <zombie_id> "<msg>"',
        }),
      );
    }

    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    const post = yield* http.request<MessagesResponse>({
      path: wsZombieMessagesPath(wsId, zombieId),
      method: "POST",
      body: { message },
      token,
    });
    if (!post.event_id) {
      return yield* Effect.fail(
        new ServerError({
          detail: "messages response missing event_id",
          suggestion: "retry; report request_id if the issue persists",
          code: "BAD_RESPONSE",
          status: 502,
          requestId: null,
        }),
      );
    }

    let outcome = yield* tailEventStream(wsId, zombieId, post.event_id, token, streamGet);
    if (outcome.kind !== "complete") {
      outcome = yield* pollEventTerminal(wsId, zombieId, post.event_id, token);
    }

    yield* renderOutcome(outcome, post.event_id, zombieId);
  });
