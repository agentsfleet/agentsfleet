/**
 * Client-side error fallbacks — surfaced when an outgoing request
 * fails without an `err.code` field (network error, transport
 * failure). Server-side UZ-* codes flow through the Effect
 * dispatcher's renderError (lib/run-effect.ts) and appear verbatim
 * on stderr / JSON envelope `.error.code`; auth-flow codes live in
 * constants/error-codes.ts (ERR_UNAUTHORIZED, ERR_TOKEN_EXPIRED).
 */

// Generic fallback used by `zombiectl doctor` and similar pre-flight
// commands when an outgoing request fails without an err.code field.
export const REQUEST_FAILED = "REQUEST_FAILED";
