import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
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
  InterruptedError,
  ServerError,
  UnexpectedError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";
import {
  readPipedMessage,
  runSteerRepl,
  shouldEnterSteerRepl,
  type ReplInputStream,
  type ReplOutputStream,
  type ReplSignalSource,
} from "../lib/repl.ts";

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
  readonly stdin?: ReplInputStream;
  readonly stdout?: ReplOutputStream;
  readonly signalSource?: ReplSignalSource;
}
export interface SteerOptions {
  readonly forceTty?: boolean;
}

const isTerminal = (status: string): boolean =>
  status === EVENT_STATUS.PROCESSED ||
  status === EVENT_STATUS.AGENT_ERROR ||
  status === EVENT_STATUS.GATE_BLOCKED;

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
  signal?: AbortSignal,
): Effect.Effect<SteerOutcome, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const url = `${config.apiUrl.replace(/\/$/, "")}${wsZombieEventsStreamPath(wsId, zombieId)}`;
    const headers = authHeaders({ token: Redacted.value(token) });
    const printLine = (line: string): void => {
      Effect.runSync(output.info(line));
    };
    let outcome: SteerOutcome = { kind: "sse_disconnected" };
    const cb = makeFrameCallback({ printLine, eventId }, (next) => {
      outcome = next;
    });

    const work = Effect.tryPromise<void, SteerOutcome>({
      try: async () => {
        await streamGet(url, headers, cb, signal ? { signal } : undefined);
      },
      catch: (err): SteerOutcome => ({
        kind: "sse_error",
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

const steerTurnEffect = (
  wsId: string,
  zombieId: string,
  message: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, CliConfig | HttpClient | Output> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
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

    let outcome = yield* tailEventStream(wsId, zombieId, post.event_id, token, streamGet, signal);
    if (signal?.aborted) {
      return yield* Effect.fail(
        new InterruptedError({
          detail: "steer interrupted",
          suggestion: "rerun the command to continue",
        }),
      );
    }
    if (outcome.kind !== "complete") {
      outcome = yield* pollEventTerminal(wsId, zombieId, post.event_id, token);
    }

    yield* renderOutcome(outcome, post.event_id, zombieId);
  });

const exitToCliError = (exit: Exit.Exit<void, CliError>): CliError => {
  if (Exit.isSuccess(exit)) return new UnexpectedError({ detail: "unexpected successful exit", suggestion: "report this" });
  const failure = Cause.findErrorOption(exit.cause);
  if (Option.isSome(failure)) return failure.value as CliError;
  return new UnexpectedError({
    detail: Cause.pretty(exit.cause),
    suggestion: "report this with the output above and the command you ran",
  });
};

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
  options: SteerOptions = {},
  deps: SteerDeps = {},
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const config = yield* CliConfig;
    const output = yield* Output;
    const streamGet = deps.streamGet ?? defaultStreamGet;
    const stdin = deps.stdin ?? (process.stdin as ReplInputStream);
    const stdout = deps.stdout ?? (process.stdout as ReplOutputStream);
    const forceTty = options.forceTty === true;

    if (!zombieId) {
      return yield* Effect.fail(
        new ValidationError({ detail: "zombie_id is required", suggestion: 'usage: zombiectl steer <zombie_id> "<message>"' }),
      );
    }

    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const enterRepl = shouldEnterSteerRepl(stdin, message, forceTty);
    if (enterRepl) {
      const turnLayer = Layer.mergeAll(
        Layer.succeed(CliConfig, config),
        Layer.succeed(HttpClient, http),
        Layer.succeed(Output, output),
      );
      const exitCode = yield* Effect.tryPromise<number, CliError>({
        try: () =>
          runSteerRepl({
            input: stdin,
            output: stdout,
            ...(deps.signalSource ? { signalSource: deps.signalSource } : {}),
            runTurn: async (line, signal) => {
              const turn = steerTurnEffect(wsId, zombieId, line, token, streamGet, signal);
              const exit = await Effect.runPromiseExit(turn.pipe(Effect.provide(turnLayer)));
              if (Exit.isFailure(exit)) throw exitToCliError(exit);
            },
          }),
        catch: (cause): CliError =>
          cause && typeof cause === "object" && "_tag" in cause
            ? (cause as CliError)
            : new UnexpectedError({
                detail: cause instanceof Error ? cause.message : String(cause),
                suggestion: "report this with the command you ran",
              }),
      });
      if (exitCode === 130) {
        return yield* Effect.fail(
          new InterruptedError({
            detail: "steer interrupted",
            suggestion: "rerun the command to continue",
          }),
        );
      }
      return;
    }

    const singleMessage = message ?? (yield* Effect.promise(() => readPipedMessage(stdin)));
    if (singleMessage.trim().length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "message is required",
          suggestion: 'usage: zombiectl steer <zombie_id> "<message>"',
        }),
      );
    }

    yield* steerTurnEffect(wsId, zombieId, singleMessage, token, streamGet);
  });
