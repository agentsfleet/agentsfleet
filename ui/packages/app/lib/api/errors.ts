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

  constructor(
    message: string,
    status: number,
    code: UzErrorCode,
    requestId?: string,
    retryAfterMs: number | null = null,
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
    this.retryAfterMs = retryAfterMs;
  }
}
