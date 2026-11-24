// The prompt `fleet steer` and `chat` share: each line a person types is one
// turn, run to its outcome before the next prompt. A failed turn is rendered
// and the prompt returns; Ctrl+C ends the session as an interrupt.

import { Effect, Exit, Layer } from "effect";
import { CliConfig } from "../../services/config.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output } from "../../services/output.ts";
import { isRecord } from "../../lib/guards.ts";
import { streamGet as defaultStreamGet } from "../../lib/sse.ts";
import { InterruptedError, UnexpectedError, type CliError } from "../../errors/index.ts";
import {
  runSteerRepl,
  type ReplInputStream,
  type ReplOutputStream,
  type ReplSignalSource,
} from "../../lib/repl.ts";
import { exitToCliError, renderCliError } from "../../lib/cli-error-render.ts";

export interface SteerDeps {
  readonly streamGet?: typeof defaultStreamGet;
  readonly stdin?: ReplInputStream;
  readonly stdout?: ReplOutputStream;
  readonly signalSource?: ReplSignalSource;
  readonly runRepl?: typeof runSteerRepl;
}

type TurnServices = CliConfig | HttpClient | Output;
/** One line's work: the turn it sends, followed to its outcome. */
export type PromptTurn = (line: string, signal?: AbortSignal) => Effect.Effect<void, CliError, TurnServices>;

const TAG_FIELD = "_tag";
const EXIT_SIGINT = 130;
const SUGGESTION_REPORT_COMMAND = "report this with the command you ran" as const;
export const SUGGESTION_INSPECT_TURNS = "inspect fleet turns before sending again; an accepted message may still run" as const;
export const DETAIL_STEER_INTERRUPTED = "steer interrupted" as const;

export const promptTurns = (
  turn: PromptTurn,
  fleetName: string | undefined,
  deps: SteerDeps,
): Effect.Effect<void, CliError, TurnServices> =>
  Effect.gen(function* () {
    // Each turn runs as its own program, so it is handed the services this one holds.
    const turnLayer = Layer.mergeAll(
      Layer.succeed(CliConfig, yield* CliConfig),
      Layer.succeed(HttpClient, yield* HttpClient),
      Layer.succeed(Output, yield* Output),
    );
    const runRepl = deps.runRepl ?? runSteerRepl;
    const exitCode = yield* Effect.tryPromise({
      try: () =>
        runRepl({
          fleetName,
          input: deps.stdin ?? (process.stdin as ReplInputStream),
          output: deps.stdout ?? (process.stdout as ReplOutputStream),
          ...(deps.signalSource ? { signalSource: deps.signalSource } : {}),
          runTurn: async (line, signal) => {
            const exit = await Effect.runPromiseExit(turn(line, signal).pipe(Effect.provide(turnLayer)));
            if (Exit.isFailure(exit)) throw exitToCliError(exit);
          },
          onTurnError: async (cause) => {
            await Effect.runPromise(renderCliError(asCliError(cause)).pipe(Effect.provide(turnLayer)));
          },
        }),
      catch: asCliError,
    });
    if (exitCode === EXIT_SIGINT) {
      return yield* Effect.fail(
        new InterruptedError({ detail: DETAIL_STEER_INTERRUPTED, suggestion: SUGGESTION_INSPECT_TURNS }),
      );
    }
  });

const asCliError = (cause: unknown): CliError =>
  isRecord(cause) && TAG_FIELD in cause
    ? (cause as unknown as CliError)
    : new UnexpectedError({
        detail: cause instanceof Error ? cause.message : String(cause),
        suggestion: SUGGESTION_REPORT_COMMAND,
      });
