// CliError taxonomy — discriminated union of every failure mode a command
// Effect may carry on its error channel. The dispatcher's shared formatter
// switches on `_tag` and the switch is exhaustive (TypeScript checks).
//
// Each variant carries:
//   - `detail`     — operator-facing message body
//   - `suggestion` — next-action hint rendered as `Suggestion: …`
//
// Server-side UZ-<CAT>-<NNN> codes map 1:1 onto AuthError / ServerError
// variants via the `code` property so support workflows still grep on the
// same identifiers.

import { Data } from "effect";
import type { AuthFlowError } from "./auth.ts";

export * from "./auth.ts";

export class AuthError extends Data.TaggedError("AuthError")<{
  readonly detail: string;
  readonly suggestion: string;
  readonly code: string;
  // Optional — populated when an AuthError is re-wrapped from a
  // ServerError (e.g. login surface re-mapping a 503 from /auth/sessions
  // so the dispatcher still emits `request_id:` for support workflows).
  readonly requestId?: string | null;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export class NetworkError extends Data.TaggedError("NetworkError")<{
  readonly detail: string;
  readonly suggestion: string;
  readonly url: string;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export class ServerError extends Data.TaggedError("ServerError")<{
  readonly detail: string;
  readonly suggestion: string;
  readonly code: string;
  readonly status: number;
  readonly requestId: string | null;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export class ValidationError extends Data.TaggedError("ValidationError")<{
  readonly detail: string;
  readonly suggestion: string;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export class ConfigError extends Data.TaggedError("ConfigError")<{
  readonly detail: string;
  readonly suggestion: string;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export class UnexpectedError extends Data.TaggedError("UnexpectedError")<{
  readonly detail: string;
  readonly suggestion: string;
}> {
  override get message(): string {
    return `${this.detail}\n  Suggestion: ${this.suggestion}`;
  }
}

export type CliError =
  | AuthError
  | NetworkError
  | ServerError
  | ValidationError
  | ConfigError
  | UnexpectedError
  // Auth-flow specializations (the AuthFlowError union from auth.ts). Kept
  // as siblings of AuthError rather than subclasses because Effect's
  // Data.TaggedError doesn't compose by inheritance and the dispatcher's
  // exit-code lookup keys directly on _tag. Each member still needs an
  // EXIT_CODE entry below or the Record type rejects the dispatcher.
  | AuthFlowError;

// Exit-code mapping. The dispatcher's exhaustive switch on `_tag`
// references this; any new variant must add a row here or the
// type-checker rejects the dispatcher.
export const EXIT_CODE: Record<CliError["_tag"], number> = {
  AuthError: 1,
  NetworkError: 2,
  ServerError: 3,
  ValidationError: 4,
  ConfigError: 5,
  UnexpectedError: 1,
  // Auth-flow specializations. All exit 1 except InterruptedError,
  // which uses the conventional SIGINT/abort code 130 so shells + CI
  // can distinguish operator-cancel from a real failure.
  InvalidSessionError: 1,
  ExpiredSessionError: 1,
  RateLimitedError: 2,
  TimeoutError: 1,
  InterruptedError: 130,
  VerificationFailedError: 1,
  DecryptError: 1,
  SessionAbortedError: 1,
  SessionConsumedError: 1,
  MeValidationError: 1,
};
