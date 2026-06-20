// CliError taxonomy — discriminated union of every failure mode a command
// Effect may carry on its error channel. The dispatcher switches on `_tag`
// and TypeScript keeps the switch exhaustive.

import {
  AUTH_FLOW_ERROR_TAG,
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
const CLI_ERROR_TAG = {
  auth: "AuthError",
  network: "NetworkError",
  server: "ServerError",
  validation: "ValidationError",
  config: "ConfigError",
  unexpected: "UnexpectedError",
} as const;

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

export class AuthError extends CliErrorBase<typeof CLI_ERROR_TAG.auth> {
  readonly code: string;
  readonly requestId: string | null | undefined;

  constructor(fields: {
    readonly detail: string;
    readonly suggestion: string;
    readonly code: string;
    readonly requestId?: string | null;
  }) {
    super(CLI_ERROR_TAG.auth, fields);
    this.code = fields.code;
    this.requestId = fields.requestId;
  }
}

export class NetworkError extends CliErrorBase<typeof CLI_ERROR_TAG.network> {
  readonly url: string;

  constructor(fields: { readonly detail: string; readonly suggestion: string; readonly url: string }) {
    super(CLI_ERROR_TAG.network, fields);
    this.url = fields.url;
  }
}

export class ServerError extends CliErrorBase<typeof CLI_ERROR_TAG.server> {
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
    super(CLI_ERROR_TAG.server, fields);
    this.code = fields.code;
    this.status = fields.status;
    this.requestId = fields.requestId;
  }
}

export class ValidationError extends CliErrorBase<typeof CLI_ERROR_TAG.validation> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(CLI_ERROR_TAG.validation, fields);
  }
}

export class ConfigError extends CliErrorBase<typeof CLI_ERROR_TAG.config> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(CLI_ERROR_TAG.config, fields);
  }
}

export class UnexpectedError extends CliErrorBase<typeof CLI_ERROR_TAG.unexpected> {
  constructor(fields: { readonly detail: string; readonly suggestion: string }) {
    super(CLI_ERROR_TAG.unexpected, fields);
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
  [CLI_ERROR_TAG.auth]: 1,
  [CLI_ERROR_TAG.network]: 2,
  [CLI_ERROR_TAG.server]: 3,
  [CLI_ERROR_TAG.validation]: 4,
  [CLI_ERROR_TAG.config]: 5,
  [CLI_ERROR_TAG.unexpected]: 1,
  // Auth-flow specializations. All exit 1 except InterruptedError,
  // which uses the conventional interrupt/abort code 130 so shells and
  // Continuous Integration (CI) can distinguish operator-cancel.
  [AUTH_FLOW_ERROR_TAG.invalidSession]: 1,
  [AUTH_FLOW_ERROR_TAG.expiredSession]: 1,
  [AUTH_FLOW_ERROR_TAG.rateLimited]: 2,
  [AUTH_FLOW_ERROR_TAG.timeout]: 1,
  [AUTH_FLOW_ERROR_TAG.interrupted]: 130,
  [AUTH_FLOW_ERROR_TAG.verificationFailed]: 1,
  [AUTH_FLOW_ERROR_TAG.decrypt]: 1,
  [AUTH_FLOW_ERROR_TAG.sessionAborted]: 1,
  [AUTH_FLOW_ERROR_TAG.sessionConsumed]: 1,
  [AUTH_FLOW_ERROR_TAG.meValidation]: 1,
};
