import { Effect, Exit, Layer, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetMessagesPath } from "../lib/api-paths.ts";
import { streamGet as defaultStreamGet } from "../lib/sse.ts";
import { EVENT_STATUS } from "../constants/event-status.ts";
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
import { exitToCliError, renderCliError } from "../lib/cli-error-render.ts";
import {
  openEventTail,
  pollEventTerminal,
  SSE_FALLBACK_TIMEOUT_SECONDS,
  STATUS_COMPLETE,
  STATUS_TIMEOUT,
  TAIL_TIMED_OUT,
  type EventTailHandle,
  type PolledSteerOutcome,
  type TailReadiness,
} from "./fleet_steer_events.ts";

const TAG_FIELD = "_tag";

const MESSAGE_PLACEHOLDER = "<message>" as const;
const TYPE_OBJECT = "object" as const;
const SUGGESTION_REPORT_COMMAND = "report this with the command you ran" as const;
const SUGGESTION_RERUN_COMMAND = "rerun the command to continue" as const;
const DETAIL_STEER_INTERRUPTED = "steer interrupted" as const;

type RenderableSteerOutcome = PolledSteerOutcome;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === TYPE_OBJECT;

const failSteerInterrupted = (): Effect.Effect<never, CliError> =>
  Effect.fail(
    new InterruptedError({
      detail: DETAIL_STEER_INTERRUPTED,
      suggestion: SUGGESTION_RERUN_COMMAND,
    }),
  );

const failMissingEventId = (): Effect.Effect<never, CliError> =>
  Effect.fail(
    new ServerError({
      detail: "messages response missing event_id",
      suggestion: "retry; report request_id if the issue persists",
      code: "BAD_RESPONSE",
      status: 502,
      requestId: null,
    }),
  );

interface MessagesResponse {
  readonly event_id?: string;
}
type StreamGetFn = typeof defaultStreamGet;
export interface SteerDeps {
  readonly streamGet?: StreamGetFn;
  readonly stdin?: ReplInputStream;
  readonly stdout?: ReplOutputStream;
  readonly signalSource?: ReplSignalSource;
  readonly runRepl?: typeof runSteerRepl;
}
export interface SteerOptions {
  readonly forceTty?: boolean;
}

// Subscribe before send: the activity channel has no replay, so the tail must
// exist before the daemon can publish the event's first frame. Acquire/release
// structurally guarantees the tail closes on every exit — success, typed
// failure, or defect — so no path (present or future) can leak the stream.
const steerTurnEffect = (
  wsId: string,
  fleetId: string,
  message: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, CliConfig | HttpClient | Output> =>
  Effect.acquireUseRelease(
    openEventTail(wsId, fleetId, token, streamGet, signal),
    (tail) => steerTurnWithTail(wsId, fleetId, message, token, tail, signal),
    (tail) => Effect.promise(() => tail.close()),
  );

const steerTurnWithTail = (
  wsId: string,
  fleetId: string,
  message: string,
  token: Redacted.Redacted<string>,
  tail: EventTailHandle,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, CliConfig | HttpClient | Output> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    // Headers-received means the server's subscription is live (it subscribes
    // before writing SSE headers), so a POST sent after this point cannot
    // race the event's first frame. Bounded: a tail that can't open inside
    // TAIL_OPEN_MAX_WAIT_MS degrades to post-then-poll.
    const readiness = yield* Effect.promise(() => tail.awaitReady());
    if (signal?.aborted) {
      // Cancelled before the send — the fleet must not execute the message.
      return yield* failSteerInterrupted();
    }
    const post = yield* http.request<MessagesResponse>({
      path: wsFleetMessagesPath(wsId, fleetId),
      method: "POST",
      body: { message },
      token,
    });
    if (!post.event_id) {
      return yield* failMissingEventId();
    }
    if (signal?.aborted) {
      return yield* failSteerInterrupted();
    }
    const outcome = yield* resolveSteerOutcome(tail, readiness, wsId, fleetId, post.event_id, token, signal);
    if (signal?.aborted) {
      return yield* failSteerInterrupted();
    }
    yield* renderOutcome(outcome, post.event_id, fleetId);
  });

// A tail that never became ready may have missed the event's opening frames;
// rendering from it could pass a truncated reply off as complete. It is
// closed unheard and the durable poll is authoritative for the outcome.
const resolveSteerOutcome = (
  tail: EventTailHandle,
  readiness: TailReadiness,
  wsId: string,
  fleetId: string,
  eventId: string,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
): Effect.Effect<RenderableSteerOutcome, CliError, CliConfig | HttpClient | Output> =>
  Effect.gen(function* () {
    if (readiness === TAIL_TIMED_OUT) {
      yield* Effect.promise(() => tail.close());
      return yield* pollEventTerminal(wsId, fleetId, eventId, token, signal);
    }
    tail.deliverEventId(eventId);
    const streamOutcome = yield* Effect.promise(() => tail.awaitOutcome());
    if (streamOutcome.kind === STATUS_COMPLETE) {
      return streamOutcome;
    }
    if (signal?.aborted) return yield* failSteerInterrupted();
    return yield* pollEventTerminal(wsId, fleetId, eventId, token, signal);
  });

const renderOutcome = (
  outcome: RenderableSteerOutcome,
  eventId: string,
  fleetId: string,
): Effect.Effect<void, CliError, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;

    if (config.jsonMode) {
      yield* output.printJson({ event_id: eventId, ...outcome });
    } else if (outcome.kind === STATUS_COMPLETE) {
      yield* output.info("");
      yield* output.success(`event ${eventId} ${outcome.status}`);
    } else if (outcome.kind === STATUS_TIMEOUT) {
      yield* output.error(
        `event ${eventId} still in flight after ${SSE_FALLBACK_TIMEOUT_SECONDS}s — check: agentsfleet events ${fleetId}`,
      );
    }

    if (outcome.kind === STATUS_COMPLETE) {
      if (outcome.status !== EVENT_STATUS.PROCESSED) {
        return yield* Effect.fail(
          new ConfigError({
            detail: `event ${eventId} terminated with status: ${outcome.status}`,
            suggestion: `inspect: agentsfleet events ${fleetId}`,
          }),
        );
      }
      return;
    }
    return yield* Effect.fail(
      new ConfigError({
        detail: `event ${eventId} did not complete (${outcome.kind})`,
        suggestion: `retry, or inspect: agentsfleet events ${fleetId}`,
      }),
    );
  });

export const steerEffectFromArgs = (
  fleetId: string | undefined,
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
    const runRepl = deps.runRepl ?? runSteerRepl;
    const forceTty = options.forceTty === true;

    if (!fleetId) {
      return yield* Effect.fail(
        new ValidationError({ detail: "fleet_id is required", suggestion: `usage: agentsfleet steer <fleet_id> ${MESSAGE_PLACEHOLDER}` }),
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
      const exitCode = yield* Effect.tryPromise({
        try: () =>
          runRepl({
            input: stdin,
            output: stdout,
            ...(deps.signalSource ? { signalSource: deps.signalSource } : {}),
            runTurn: async (line, signal) => {
              const turn = steerTurnEffect(wsId, fleetId, line, token, streamGet, signal);
              const exit = await Effect.runPromiseExit(turn.pipe(Effect.provide(turnLayer)));
              if (Exit.isFailure(exit)) throw exitToCliError(exit);
            },
            onTurnError: async (cause) => {
              const err = isRecord(cause) && TAG_FIELD in cause
                ? (cause as unknown as CliError)
                : new UnexpectedError({
                    detail: cause instanceof Error ? cause.message : String(cause),
                    suggestion: SUGGESTION_REPORT_COMMAND,
                  });
              await Effect.runPromise(renderCliError(err).pipe(Effect.provide(turnLayer)));
            },
          }),
        catch: (cause): CliError =>
          isRecord(cause) && TAG_FIELD in cause
            ? (cause as unknown as CliError)
            : new UnexpectedError({
                detail: cause instanceof Error ? cause.message : String(cause),
                suggestion: SUGGESTION_REPORT_COMMAND,
              }),
      });
      if (exitCode === 130) {
        return yield* Effect.fail(
          new InterruptedError({
            detail: DETAIL_STEER_INTERRUPTED,
            suggestion: SUGGESTION_RERUN_COMMAND,
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
          suggestion: `usage: agentsfleet steer <fleet_id> ${MESSAGE_PLACEHOLDER}`,
        }),
      );
    }

    yield* steerTurnEffect(wsId, fleetId, singleMessage, token, streamGet);
  });
