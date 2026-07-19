// Auth-flow tagged error classes. Each variant owns a stable `_tag` so
// Effect.catchTag and the shared renderer can classify login failures.

const SUGGESTION_PREFIX = "\n  Suggestion: " as const;
export const ERR_UNAUTHORIZED = "UZ-AUTH-002" as const;
export const AUTH_FLOW_ERROR_TAG = {
  invalidSession: "InvalidSessionError",
  expiredSession: "ExpiredSessionError",
  rateLimited: "RateLimitedError",
  timeout: "TimeoutError",
  interrupted: "InterruptedError",
  verificationFailed: "VerificationFailedError",
  decrypt: "DecryptError",
  sessionAborted: "SessionAbortedError",
  sessionConsumed: "SessionConsumedError",
  meValidation: "MeValidationError",
} as const;

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

export class InvalidSessionError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.invalidSession> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.invalidSession, fields);
  }
}

export class ExpiredSessionError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.expiredSession> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.expiredSession, fields);
  }
}

export class RateLimitedError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.rateLimited> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.rateLimited, fields);
  }
}

export class TimeoutError extends AuthFlowErrorBase<typeof AUTH_FLOW_ERROR_TAG.timeout> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(AUTH_FLOW_ERROR_TAG.timeout, fields);
  }
}

export class InterruptedError extends AuthFlowErrorBase<typeof AUTH_FLOW_ERROR_TAG.interrupted> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(AUTH_FLOW_ERROR_TAG.interrupted, fields);
  }
}

export class VerificationFailedError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.verificationFailed> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.verificationFailed, fields);
  }
}

export class DecryptError extends AuthFlowErrorBase<typeof AUTH_FLOW_ERROR_TAG.decrypt> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(AUTH_FLOW_ERROR_TAG.decrypt, fields);
  }
}

export class SessionAbortedError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.sessionAborted> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.sessionAborted, fields);
  }
}

export class SessionConsumedError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.sessionConsumed> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.sessionConsumed, fields);
  }
}

export class MeValidationError extends AuthFlowRequestError<typeof AUTH_FLOW_ERROR_TAG.meValidation> {
  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly requestId?: string | null }) {
    super(AUTH_FLOW_ERROR_TAG.meValidation, fields);
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
