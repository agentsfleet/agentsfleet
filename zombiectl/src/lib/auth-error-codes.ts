// Device-flow authentication error codes — the public `code` strings
// emitted on AuthError when each failure mode hits. Single source of
// truth (RULE UFS) so the CLI, the auth spec's error taxonomy, and any
// future acceptance suite all reference the same identifier.
//
// These ride on AuthError.code rather than spawning new tagged classes:
// the substrate's existing AuthError already carries detail/suggestion/
// code/requestId, the dispatcher's exit-code map keys on the class tag
// (`AuthError` → 1) which is correct for every variant below, and
// login.ts / auth.ts already mint AuthError with bespoke code strings.

export const AUTH_CODE_INVALID_SESSION = "InvalidSession" as const;
export const AUTH_CODE_EXPIRED_SESSION = "ExpiredSession" as const;
export const AUTH_CODE_RATE_LIMITED = "RateLimited" as const;
export const AUTH_CODE_TIMEOUT = "Timeout" as const;
export const AUTH_CODE_INTERRUPTED = "Interrupted" as const;
export const AUTH_CODE_VERIFICATION_FAILED = "VerificationFailed" as const;
export const AUTH_CODE_DECRYPT_ERROR = "DecryptError" as const;
export const AUTH_CODE_SESSION_ABORTED = "SessionAborted" as const;
export const AUTH_CODE_SESSION_CONSUMED = "SessionConsumed" as const;
export const AUTH_CODE_ME_VALIDATION = "MeValidation" as const;
export const AUTH_CODE_NO_INPUT_ABORT = "NoInputAbort" as const;

export type AuthErrorCode =
  | typeof AUTH_CODE_INVALID_SESSION
  | typeof AUTH_CODE_EXPIRED_SESSION
  | typeof AUTH_CODE_RATE_LIMITED
  | typeof AUTH_CODE_TIMEOUT
  | typeof AUTH_CODE_INTERRUPTED
  | typeof AUTH_CODE_VERIFICATION_FAILED
  | typeof AUTH_CODE_DECRYPT_ERROR
  | typeof AUTH_CODE_SESSION_ABORTED
  | typeof AUTH_CODE_SESSION_CONSUMED
  | typeof AUTH_CODE_ME_VALIDATION
  | typeof AUTH_CODE_NO_INPUT_ABORT;
