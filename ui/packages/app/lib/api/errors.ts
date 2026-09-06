export type UzErrorCode = string;

/**
 * The status a client-side timeout is reported under. 408 is the closest
 * standard meaning (the request did not complete in time). Declared beside
 * `ApiError` so a client component can read it without importing the retry
 * policy, whose dependency is server-only.
 */
export const HTTP_STATUS_REQUEST_TIMEOUT = 408;

/**
 * The `ApiError.code` a client-side request timeout carries — the retry
 * layer's own class, distinct from any `UZ-` wire code. The transport, the
 * classifier and the operator copy all read it, so it is declared once here.
 */
export const RETRY_CODE_TIMEOUT = "TIMEOUT";

/** Where the client-error class begins and ends. */
const HTTP_STATUS_CLIENT_ERROR_FLOOR = 400;
const HTTP_STATUS_SERVER_ERROR_FLOOR = 500;

/**
 * Whether a failed write's outcome is settled by its status: a client-class
 * refusal other than a timeout means the server saw the request and said no,
 * so nothing changed. Anything else — no status at all (a transport fault),
 * a timeout, a server or gateway error — leaves the server's state in doubt,
 * and a surface that painted the write optimistically re-reads before it
 * trusts its own rollback.
 */
export function isDefiniteRefusal(status: number | undefined): boolean {
  if (status === undefined || status === HTTP_STATUS_REQUEST_TIMEOUT) return false;
  return status >= HTTP_STATUS_CLIENT_ERROR_FLOOR && status < HTTP_STATUS_SERVER_ERROR_FLOOR;
}

export class ApiError extends Error {
  status: number;
  code: UzErrorCode;
  requestId: string | undefined;
  /**
   * Server-supplied Retry-After value in milliseconds when present,
   * else `null`. Captured at the single-attempt boundary in `client.ts`
   * while `Response.headers` is still in scope; the retry policy in
   * `retry.ts` reads it off the error so the 429/Retry-After floor does
   * not depend on the parsed body's shape.
   */
  retryAfterMs: number | null;
  /**
   * The resource's current ETag, present on a 412 Precondition Failed so an
   * optimistic-concurrency editor can rebase its edit without a second GET
   * (REST guide §4). Null on every other status.
   */
  etag: string | null;

  constructor(
    message: string,
    status: number,
    code: UzErrorCode,
    requestId?: string,
    retryAfterMs: number | null = null,
    etag: string | null = null,
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
    this.retryAfterMs = retryAfterMs;
    this.etag = etag;
  }
}

/**
 * A request the caller abandoned — a navigation away, a superseded search
 * keystroke, a React effect cleanup.
 *
 * Deliberately NOT an `ApiError`: nothing failed, the server may never have
 * been asked, and there is no status, no request id, and no error code to
 * carry. Callers `instanceof` this to drop the result silently instead of
 * surfacing a toast for a page the user already left.
 *
 * It also carries no `UZ-` code on purpose. Those are the wire registry's, and
 * this condition never reaches the wire.
 */
export class RequestCancelledError extends Error {
  constructor(public readonly path: string) {
    super(`request to ${path} was cancelled`);
    this.name = "RequestCancelledError";
  }
}
