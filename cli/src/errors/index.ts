// CliError taxonomy — discriminated union of every failure mode a command
// Effect may carry on its error channel. The dispatcher switches on `_tag`
// and TypeScript keeps the switch exhaustive.

import {
  DecryptError,
  ExpiredSessionError,
  InterruptedError,
  InvalidSessionError,
  MeValidationError,
  RateLimitedError,
  SessionAbortedError,
  SessionConsumedError,
  TimeoutError,
  VerificationFailedError,
  type AuthFlowError,
} from "./auth.ts";

export {
  DecryptError,
  ExpiredSessionError,
  InterruptedError,
  InvalidSessionError,
  MeValidationError,
  RateLimitedError,
  SessionAbortedError,
  SessionConsumedError,
  TimeoutError,
  VerificationFailedError,
  type AuthFlowError,
};

const SUGGESTION_PREFIX = "\n  Suggestion: " as const;

abstract class CliErrorBase<Tag extends string> extends Error {
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

export class AuthError extends CliErrorBase<"AuthError"> {
  readonly code: string;
  readonly requestId: string | null | undefined;

  constructor(fields: {
    readonly detail: string;
    readonly suggestion: string;
    readonly code: string;
    readonly requestId?: string | null;
  }) {
    super("AuthError", fields);
    this.code = fields.code;
    this.requestId = fields.requestId;
  }
}

export class NetworkError extends CliErrorBase<"NetworkError"> {
  readonly url: string;

  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly url: string }) {
    super("NetworkError", fields);
    this.url = fields.url;
  }
}

export class ServerError extends CliErrorBase<"ServerError"> {
  readonly code: string;
  readonly status: number;
  readonly requestId: string | null;

  constructor(fields: {
    readonly detail: string;
    readonly suggestion: string;
    readonly code: string;
    readonly status: number;
    readonly requestId: string | null;
  }) {
    super("ServerError", fields);
    this.code = fields.code;
    this.status = fields.status;
    this.requestId = fields.requestId;
  }
}

export class ValidationError extends CliErrorBase<"ValidationError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("ValidationError", fields);
  }
}

export class ConfigError extends CliErrorBase<"ConfigError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("ConfigError", fields);
  }
}

export class UnexpectedError extends CliErrorBase<"UnexpectedError"> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super("UnexpectedError", fields);
  }
}

export type CliError =
  | AuthError
  | NetworkError
  | ServerError
  | ValidationError
  | ConfigError
  | UnexpectedError
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
  // which uses the conventional interrupt/abort code 130 so shells and
  // Continuous Integration (CI) can distinguish operator-cancel.
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
