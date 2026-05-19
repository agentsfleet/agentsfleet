// Auth-flow tagged error classes — Supabase-parity shape (mirrors
// ~/Projects/oss/cli/apps/cli/src/next/commands/login/login.errors.ts).
// Each variant is its own tagged class so the dispatcher's exit-code map
// keys on a unique `_tag` and the formatter switch on `_tag` is
// exhaustive at compile time. Replaces the earlier string-code-on-
// AuthError shape from auth-error-codes.ts.
//
// All variants carry { detail, suggestion } at minimum. ServerError-
// derived variants additionally carry an optional requestId so the
// dispatcher can render `request_id:` alongside the detail for support
// workflows — mirrors AuthError's existing requestId convention.

import { Data } from "effect";

interface BaseFields {
  readonly detail: string;
  readonly suggestion: string;
}

interface WithRequestId extends BaseFields {
  readonly requestId?: string | null;
}

const baseMessage = (e: BaseFields): string =>
  `${e.detail}\n  Suggestion: ${e.suggestion}`;

export class InvalidSessionError extends Data.TaggedError("InvalidSessionError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class ExpiredSessionError extends Data.TaggedError("ExpiredSessionError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class RateLimitedError extends Data.TaggedError("RateLimitedError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class TimeoutError extends Data.TaggedError("TimeoutError")<BaseFields> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class InterruptedError extends Data.TaggedError("InterruptedError")<BaseFields> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class VerificationFailedError extends Data.TaggedError("VerificationFailedError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class DecryptError extends Data.TaggedError("DecryptError")<BaseFields> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class SessionAbortedError extends Data.TaggedError("SessionAbortedError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class SessionConsumedError extends Data.TaggedError("SessionConsumedError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export class MeValidationError extends Data.TaggedError("MeValidationError")<WithRequestId> {
  override get message(): string {
    return baseMessage(this);
  }
}

export type AuthFlowError =
  | InvalidSessionError
  | ExpiredSessionError
  | RateLimitedError
  | TimeoutError
  | InterruptedError
  | VerificationFailedError
  | DecryptError
  | SessionAbortedError
  | SessionConsumedError
  | MeValidationError;
