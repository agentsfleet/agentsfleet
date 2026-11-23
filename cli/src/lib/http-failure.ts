// How a failed request is read: whether the failure is one a person may simply
// run again, and whether the connection itself broke. The CLI sends each
// request once; these only shape the error the operator reads.

import { ApiError } from "./http.ts";
import { isRecord } from "./guards.ts";

// Statuses that answer for the moment rather than the request (RFC 9110:
// 408, 425, 429, 502-504), so running the command again may succeed.
const RETRYABLE_STATUSES = new Set([408, 425, 429, 502, 503, 504]);

/**
 * Codes that prove the connection itself broke after the request was on the
 * wire. A code the runtime attaches for something else — `ERR_INVALID_URL`,
 * `ERR_INVALID_ARG_VALUE`, a certificate failure such as
 * `DEPTH_ZERO_SELF_SIGNED_CERT` — is not one, and a certificate failure in
 * particular must reach the operator in its own words rather than as a
 * connectivity hint.
 */
export const DROP_CODES: ReadonlySet<string> = new Set([
  "ECONNRESET",
  "ECONNABORTED",
  "EPIPE",
  "ETIMEDOUT",
  "UND_ERR_SOCKET",
  "ConnectionClosed",
]);

// Socket and resolver codes that prove the request never left. Node's fetch
// puts them on the error's `cause` (`UND_ERR_CONNECT_TIMEOUT` is undici's);
// Bun's puts them on the error itself (`ConnectionRefused` is Bun's).
export const PRE_SEND_CODES: ReadonlySet<string> = new Set([
  "ECONNREFUSED",
  "ENOTFOUND",
  "EAI_AGAIN",
  "UND_ERR_CONNECT_TIMEOUT",
  "ConnectionRefused",
]);

// The server says running it again will not help: `retry_after_seconds: 0`.
function hasRetryOptOut(body: unknown): boolean {
  if (!isRecord(body)) return false;
  const errField = (body as { error?: unknown }).error;
  if (!isRecord(errField)) return false;
  return (errField as { retry_after_seconds?: unknown }).retry_after_seconds === 0;
}

export function isTransientResponse(error: ApiError): boolean {
  return error.code === "TIMEOUT" || (error.status !== undefined &&
    RETRYABLE_STATUSES.has(error.status) && !hasRetryOptOut(error.body));
}

function codeOf(value: unknown): string | undefined {
  if (!(value instanceof Object) || !("code" in value)) return undefined;
  return typeof value.code === "string" ? value.code : undefined;
}

// The code is on the error under Bun and on its cause under Node.
export function socketCode(err: Error): string | undefined {
  return codeOf(err) ?? codeOf(err.cause);
}
