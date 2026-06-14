import { Cause, Effect, Exit, Option } from "effect";
import { Output } from "../services/output.ts";
import {
  UnexpectedError,
  type CliError,
} from "../errors/index.ts";

export const exitToCliError = (exit: Exit.Exit<void, CliError>): CliError => {
  if (Exit.isSuccess(exit)) {
    return new UnexpectedError({ detail: "unexpected successful exit", suggestion: "report this" });
  }
  const failure = Cause.findErrorOption(exit.cause);
  if (Option.isSome(failure)) return failure.value as CliError;
  return new UnexpectedError({
    detail: Cause.pretty(exit.cause),
    suggestion: "report this with the output above and the command you ran",
  });
};

export const renderCliError = (err: CliError): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    if (err._tag === "ServerError" || err._tag === "AuthError") {
      const requestId = err.requestId ? `\nrequest_id: ${err.requestId}` : "";
      yield* output.error(`${err.code} ${err.detail}\n  Suggestion: ${err.suggestion}${requestId}`);
      return;
    }
    yield* output.error(err.message);
  });
