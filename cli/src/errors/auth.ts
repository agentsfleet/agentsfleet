// Auth-flow tagged error classes. Each variant owns a stable `_tag` so
// Effect.catchTag and the shared renderer can classify login failures.

const SUGGESTION_PREFIX = "\n  Suggestion: " as const;

abstract class AuthFlowErrorBase<Tag extends string> extends Error {
  readonly _tag: Tag;
  readonly detail: string;
  readonly suggestion: string;

  protected constructor(
    tag: Tag,
    fields: { readonly detail: string; readonly suggestion: string },
  ) {
    super(`${fields.detail}${SUGGESTION_PREFIX}${fields.suggestion}`);
    this.name = tag;
    this._tag = tag;
    this.detail = fields.detail;
    this.suggestion = fields.suggestion;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

abstract class AuthFlowRequestError<Tag extends string> extends AuthFlowErrorBase<Tag> {
  readonly requestId: string | null | undefined;

  protected constructor(
    tag: Tag,
    fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null },
  ) {
    super(tag, fields);
    this.requestId = fields.requestId;
  }
}

export class InvalidSessionError extends AuthFlowRequestError<"InvalidSessionError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("InvalidSessionError", fields);
  }
}

export class ExpiredSessionError extends AuthFlowRequestError<"ExpiredSessionError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("ExpiredSessionError", fields);
  }
}

export class RateLimitedError extends AuthFlowRequestError<"RateLimitedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("RateLimitedError", fields);
  }
}

export class TimeoutError extends AuthFlowErrorBase<"TimeoutError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("TimeoutError", fields);
  }
}

export class InterruptedError extends AuthFlowErrorBase<"InterruptedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("InterruptedError", fields);
  }
}

export class VerificationFailedError extends AuthFlowRequestError<"VerificationFailedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("VerificationFailedError", fields);
  }
}

export class DecryptError extends AuthFlowErrorBase<"DecryptError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("DecryptError", fields);
  }
}

export class SessionAbortedError extends AuthFlowRequestError<"SessionAbortedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("SessionAbortedError", fields);
  }
}

export class SessionConsumedError extends AuthFlowRequestError<"SessionConsumedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("SessionConsumedError", fields);
  }
}

export class MeValidationError extends AuthFlowRequestError<"MeValidationError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super("MeValidationError", fields);
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
