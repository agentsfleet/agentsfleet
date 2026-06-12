import { Effect, Exit, Layer, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsZombieMessagesPath } from "../lib/api-paths.ts";
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
  pollEventTerminal,
  SSE_FALLBACK_TIMEOUT_SECONDS,
  STATUS_COMPLETE,
  STATUS_TIMEOUT,
  tailEventStream,
  type PolledSteerOutcome,
} from "./zombie_steer_events.ts";

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

    const streamOutcome = yield* tailEventStream(wsId, zombieId, post.event_id, token, streamGet, signal);
    let outcome: RenderableSteerOutcome;
    if (streamOutcome.kind === STATUS_COMPLETE) {
      outcome = streamOutcome;
    } else {
      if (signal?.aborted) return yield* failSteerInterrupted();
      outcome = yield* pollEventTerminal(wsId, zombieId, post.event_id, token, signal);
    }
    if (signal?.aborted) {
      return yield* failSteerInterrupted();
    }
    yield* renderOutcome(outcome, post.event_id, zombieId);
  });

const renderOutcome = (
  outcome: RenderableSteerOutcome,
  eventId: string,
  zombieId: string,
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
        `event ${eventId} still in flight after ${SSE_FALLBACK_TIMEOUT_SECONDS}s — check: agentsfleet events ${zombieId}`,
      );
    }

    if (outcome.kind === STATUS_COMPLETE) {
      if (outcome.status !== EVENT_STATUS.PROCESSED) {
        return yield* Effect.fail(
          new ConfigError({
            detail: `event ${eventId} terminated with status: ${outcome.status}`,
            suggestion: `inspect: agentsfleet events ${zombieId}`,
          }),
        );
      }
      return;
    }
    return yield* Effect.fail(
      new ConfigError({
        detail: `event ${eventId} did not complete (${outcome.kind})`,
        suggestion: `retry, or inspect: agentsfleet events ${zombieId}`,
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
    const runRepl = deps.runRepl ?? runSteerRepl;
    const forceTty = options.forceTty === true;

    if (!zombieId) {
      return yield* Effect.fail(
        new ValidationError({ detail: "zombie_id is required", suggestion: `usage: agentsfleet steer <zombie_id> ${MESSAGE_PLACEHOLDER}` }),
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
              const turn = steerTurnEffect(wsId, zombieId, line, token, streamGet, signal);
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
          suggestion: `usage: agentsfleet steer <zombie_id> ${MESSAGE_PLACEHOLDER}`,
        }),
      );
    }

    yield* steerTurnEffect(wsId, zombieId, singleMessage, token, streamGet);
  });
