import { Effect, Redacted } from "effect";
import { readSteeredWork } from "./steer-work.ts";
import { resolveSteerTarget } from "./steer-target.ts";
import {
  DETAIL_STEER_INTERRUPTED,
  promptTurns,
  SUGGESTION_INSPECT_TURNS,
  type SteerDeps,
} from "./steer-prompt.ts";
import { CliConfig } from "../../services/config.ts";
import { Credentials } from "../../services/credentials.ts";
import { HttpClient } from "../../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../../services/output.ts";
import { Workspaces } from "../../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "../workspace/guards.ts";
import { readFleetAddress } from "../../lib/fleet-address.ts";
import { wsFleetMessagesPath } from "../../lib/api-paths.ts";
import { streamGet as defaultStreamGet } from "../../lib/sse.ts";
import { TURN_STATUS } from "../../constants/turn-status.ts";
import {
  ConfigError,
  InterruptedError,
  ServerError,
  ValidationError,
  type CliError,
} from "../../errors/index.ts";
import { readPipedMessage, shouldEnterSteerRepl, type ReplInputStream } from "../../lib/repl.ts";
import {
  DISPOSITION_FOLDED,
  DISPOSITION_QUEUED,
  DISPOSITION_STARTED,
  decodeMessagesResponse,
  type MessagesResponse,
} from "./steer-placement.ts";
import {
  openTurnTail,
  pollTurnTerminal,
  STATUS_COMPLETE,
  TAIL_TIMED_OUT,
  type TurnTailHandle,
  type PolledSteerOutcome,
  type TailReadiness,
} from "./steer-turns.ts";

export type { SteerDeps } from "./steer-prompt.ts";

const STEER_OUTCOME = "Steer outcome" as const;
// Where the daemon placed the message. `started` prints nothing: the run that
// follows is the message's own.
const LINE_FOLDED = "added to the current run" as const;
const LINE_QUEUED = "queued behind the current run" as const;
const MESSAGE_PLACEHOLDER = "<message>" as const;

type RenderableSteerOutcome = PolledSteerOutcome;
type StreamGetFn = typeof defaultStreamGet;
type TurnServices = CliConfig | HttpClient | Output;

/** A turn the daemon has admitted: its id, and where it placed the message. */
export type AdmittedTurn = MessagesResponse & { readonly turn_id: string };

const failSteerInterrupted = (): Effect.Effect<never, CliError> =>
  Effect.fail(
    new InterruptedError({
      detail: DETAIL_STEER_INTERRUPTED,
      suggestion: SUGGESTION_INSPECT_TURNS,
    }),
  );

const failMissingTurnId = (): Effect.Effect<never, CliError> =>
  Effect.fail(
    new ServerError({
      detail: "messages response missing turn_id",
      suggestion: "retry; report request_id if the issue persists",
      code: "BAD_RESPONSE",
      status: 502,
      requestId: null,
    }),
  );

// The daemon accepted the message but its answer is not one this CLI reads,
// so the message may still run and the guidance is the same as for a stall.
const failUnreadableResponse = (): CliError =>
  new ServerError({
    detail: "messages response did not match the supported API shape",
    suggestion: SUGGESTION_INSPECT_TURNS,
    code: "BAD_RESPONSE",
    status: 502,
    requestId: null,
  });

export interface SteerOptions {
  readonly forceTty?: boolean;
}

// Subscribe before send: the activity channel has no replay, so the tail must
// exist before the daemon can publish the turn's first frame. Acquire/release
// structurally guarantees the tail closes on every exit — success, typed
// failure, or defect — so no path (present or future) can leak the stream.
export const steerTurnEffect = (
  wsId: string,
  fleetId: string,
  message: string,
  token: Redacted.Redacted<string>,
  streamGet: StreamGetFn,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, TurnServices> =>
  Effect.acquireUseRelease(
    openTurnTail(wsId, fleetId, token, streamGet, signal),
    (tail) => steerTurnWithTail(wsId, fleetId, message, token, tail, signal),
    (tail) => Effect.promise(() => tail.close()),
  );

const steerTurnWithTail = (
  wsId: string,
  fleetId: string,
  message: string,
  token: Redacted.Redacted<string>,
  tail: TurnTailHandle,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, TurnServices> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    // Headers-received means the server's subscription is live (it subscribes
    // before writing SSE headers), so a POST sent after this point cannot
    // race the turn's first frame. Bounded: a tail that can't open inside
    // TAIL_OPEN_MAX_WAIT_MS degrades to post-then-poll.
    const readiness = yield* Effect.promise(() => tail.awaitReady());
    if (signal?.aborted) {
      // Cancelled before the send — the fleet must not execute the message.
      return yield* failSteerInterrupted();
    }
    const answer = yield* http.request<unknown>({
      path: wsFleetMessagesPath(wsId, fleetId),
      method: "POST",
      body: { message },
      token,
      signal,
    }).pipe(
      Effect.catch((error) => signal?.aborted ? failSteerInterrupted() : Effect.fail(error)),
    );
    const post = yield* decodeMessagesResponse(answer).pipe(Effect.mapError(failUnreadableResponse));
    if (!post.turn_id) {
      return yield* failMissingTurnId();
    }
    if (signal?.aborted) {
      return yield* failSteerInterrupted();
    }
    const output = yield* Output;
    if (output.format === OUTPUT_FORMAT.text) {
      const placement = post.disposition ?? DISPOSITION_STARTED;
      if (placement === DISPOSITION_FOLDED) yield* output.info(LINE_FOLDED);
      if (placement === DISPOSITION_QUEUED) yield* output.info(LINE_QUEUED);
    }
    // A folded message keeps its own turn: the run it joined answers it
    // there when that run reports, so the CLI waits on the message's own id.
    yield* settleTurn(tail, readiness, wsId, fleetId, { ...post, turn_id: post.turn_id }, token, signal);
  });

// Waits out the turn's outcome, then prints it. The poll stops short of a terminal status only when the caller's
// signal ended the wait.
const settleTurn = (
  tail: TurnTailHandle,
  readiness: TailReadiness,
  wsId: string,
  fleetId: string,
  turn: AdmittedTurn,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
): Effect.Effect<void, CliError, TurnServices> =>
  Effect.gen(function* () {
    const outcome = yield* resolveSteerOutcome(tail, readiness, wsId, fleetId, turn.turn_id, token, signal);
    if (signal?.aborted || outcome.kind !== STATUS_COMPLETE) {
      return yield* failSteerInterrupted();
    }
    yield* readSteeredWork(wsId, fleetId, turn.turn_id, token, signal);
    yield* renderOutcome(outcome, fleetId, turn);
  });

// A tail that never became ready may have missed the turn's opening frames;
// rendering from it could pass a truncated reply off as complete. It is
// closed unheard and the durable poll is authoritative for the outcome.
const resolveSteerOutcome = (
  tail: TurnTailHandle,
  readiness: TailReadiness,
  wsId: string,
  fleetId: string,
  turnId: string,
  token: Redacted.Redacted<string>,
  signal?: AbortSignal,
): Effect.Effect<RenderableSteerOutcome, CliError, CliConfig | HttpClient | Output> =>
  Effect.gen(function* () {
    if (readiness === TAIL_TIMED_OUT) {
      yield* Effect.promise(() => tail.close());
      return yield* pollTurnTerminal(wsId, fleetId, turnId, token, signal, tail.presentation);
    }
    tail.deliverTurnId(turnId);
    const streamOutcome = yield* Effect.promise(() => tail.awaitOutcome());
    if (streamOutcome.kind === STATUS_COMPLETE) {
      return streamOutcome;
    }
    if (signal?.aborted) return yield* failSteerInterrupted();
    return yield* pollTurnTerminal(wsId, fleetId, turnId, token, signal, tail.presentation);
  });

const renderOutcome = (
  outcome: Extract<RenderableSteerOutcome, { readonly kind: typeof STATUS_COMPLETE }>,
  fleetId: string,
  turn: AdmittedTurn,
): Effect.Effect<void, CliError, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const turnId = turn.turn_id;

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(STEER_OUTCOME, {
        turn_id: turnId,
        ...(turn.disposition ? { disposition: turn.disposition } : {}),
        ...(turn.folded_into_turn_id ? { folded_into_turn_id: turn.folded_into_turn_id } : {}),
        ...outcome,
      });
    } else {
      yield* output.info("");
      yield* output.success(`turn ${turnId} ${outcome.status}`);
    }

    if (outcome.status !== TURN_STATUS.PROCESSED) {
      return yield* Effect.fail(
        new ConfigError({
          detail: `turn ${turnId} terminated with status: ${outcome.status}`,
          suggestion: `inspect: agentsfleet fleet turns ${fleetId}`,
        }),
      );
    }
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
    const streamGet = deps.streamGet ?? defaultStreamGet;
    const stdin = deps.stdin ?? (process.stdin as ReplInputStream);

    if (!fleetId) {
      return yield* Effect.fail(
        new ValidationError({ detail: "fleet_id is required", suggestion: `usage: agentsfleet fleet steer <@name|fleet_id> ${MESSAGE_PLACEHOLDER}` }),
      );
    }

    const wsId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    // Once, before any tail opens: the prompt's every turn and a single message
    // both go to the fleet a `@name` names.
    const target = yield* resolveSteerTarget(wsId, fleetId, token);
    if (shouldEnterSteerRepl(stdin, message, options.forceTty === true)) {
      return yield* promptTurns(
        (line, signal) => steerTurnEffect(wsId, target, line, token, streamGet, signal),
        readFleetAddress(fleetId)?.name,
        { ...deps, stdin },
      );
    }

    const singleMessage = message ?? (yield* Effect.promise(() => readPipedMessage(stdin)));
    if (singleMessage.trim().length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "message is required",
          suggestion: `usage: agentsfleet fleet steer <@name|fleet_id> ${MESSAGE_PLACEHOLDER}`,
        }),
      );
    }

    yield* steerTurnEffect(wsId, target, singleMessage, token, streamGet);
  });
