// Auth-flow tagged error classes — Supabase-parity shape (mirrors
// ~/Projects/oss/cli/apps/cli/src/next/commands/login/login.errors.ts).
// Each variant is its own tagged class so the dispatcher's exit-code map
// keys on a unique `_tag` and the formatter switch on `_tag` is
// exhaustive at compile time. Built via factories (the reference idiom)
// rather than 10 hand-written class bodies — one shared `message` getter
// per field-shape instead of ten.
//
// All variants carry { detail, suggestion } at minimum. ServerError-
// derived variants additionally carry an optional requestId so the
// dispatcher can render `request_id:` alongside the detail for support
// workflows — mirrors AuthError's existing requestId convention. The
// reference has a single field shape; zombie needs two (with/without
// requestId), hence two factories over the reference's one.

import { Data } from "effect";

interface BaseFields { readonly detail: string; readonly suggestion: string; }
interface WithRequestId extends BaseFields { readonly requestId?: string | null; }

const baseMessage = (e: BaseFields): string =>
  `${e.detail}\n  Suggestion: ${e.suggestion}`;

function baseError<Tag extends string>(tag: Tag) {
  return class extends Data.TaggedError(tag)<BaseFields> {
    override get message(): string {
      return baseMessage(this);
    }
  };
}

function reqIdError<Tag extends string>(tag: Tag) {
  return class extends Data.TaggedError(tag)<WithRequestId> {
    override get message(): string {
      return baseMessage(this);
    }
  };
}

export class InvalidSessionError extends reqIdError("InvalidSessionError") {}
export class ExpiredSessionError extends reqIdError("ExpiredSessionError") {}
export class RateLimitedError extends reqIdError("RateLimitedError") {}
export class TimeoutError extends baseError("TimeoutError") {}
export class InterruptedError extends baseError("InterruptedError") {}
export class VerificationFailedError extends reqIdError("VerificationFailedError") {}
export class DecryptError extends baseError("DecryptError") {}
export class SessionAbortedError extends reqIdError("SessionAbortedError") {}
export class SessionConsumedError extends reqIdError("SessionConsumedError") {}
export class MeValidationError extends reqIdError("MeValidationError") {}

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
