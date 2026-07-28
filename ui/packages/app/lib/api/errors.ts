export type UzErrorCode = string;

export class ApiError extends Error {
  status: number;
  code: UzErrorCode;
  requestId: string | undefined;
  /**
   * Server-supplied Retry-After value in milliseconds when present,
   * else `null`. Captured at the `request()` boundary while
   * `Response.headers` is still in scope; `requestWithRetry` reads
   * this directly so the 429/Retry-After floor does not depend on
   * the parsed body's shape.
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
