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
