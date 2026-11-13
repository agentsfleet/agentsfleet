import { Cause, Effect, Exit, Option } from "effect";
import { Output, OUTPUT_FORMAT } from "../services/output.ts";
import { REJECTION_CODE, USAGE_PREFIX } from "../constants/rejection.ts";
import type { ErrorBlock } from "../output/error-block.ts";
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

// A handler's own `usage: …` suggestion is a usage line, so it gets that label.
const suggestionFields = (suggestion: string): ErrorBlock["fields"] =>
  suggestion.startsWith(USAGE_PREFIX)
    ? { usage: suggestion.slice(USAGE_PREFIX.length) }
    : { suggestion };

const textFields = (err: CliError): ErrorBlock["fields"] => {
  if (err._tag === "ServerError" || err._tag === "AuthError") {
    return { code: err.code, ...suggestionFields(err.suggestion), ...(err.requestId ? { request: err.requestId } : {}) };
  }
  const code = err._tag === "ValidationError" ? REJECTION_CODE.invalidArgument : err._tag;
  return { code, ...suggestionFields(err.suggestion) };
};

export const renderCliError = (err: CliError): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    if (output.format === OUTPUT_FORMAT.json) {
      const code = err._tag === "ServerError" || err._tag === "AuthError" ? err.code
        : err._tag === "ValidationError" ? REJECTION_CODE.invalidArgument : err._tag;
      yield* output.printJsonErr({error: {code, message: err.detail, suggestion: err.suggestion,
        ...("requestId" in err && err.requestId ? {request_id: err.requestId} : {})}});
      return;
    }
    yield* output.error(err.detail, { fields: textFields(err) });
  });
