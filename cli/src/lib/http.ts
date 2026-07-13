
const APIERROR = "ApiError" as const;
const TYPE_OBJECT = "object" as const;
const TYPE_STRING = "string" as const;
const MS_PER_SECOND = 1000 as const;

// HTTP transport: fetch wrapper, JSON envelope unwrap, AbortController-backed
// timeout. Every non-OK response surfaces as ApiError; the caller never sees a
// raw Response. The retry-with-backoff layer lives in http-retry.ts.

const DEFAULT_TIMEOUT_MS = 15000;

export interface ApiErrorDetails {
  status?: number;
  code?: string;
  requestId?: string | null;
  body?: unknown;
  retryAfterMs?: number | null;
}

export interface ProblemDetails {
  readonly code?: string | undefined;
  readonly message?: string | undefined;
  readonly requestId?: string | undefined;
  readonly missingSecrets?: ReadonlyArray<string> | undefined;
}

export class ApiError extends Error {
  override readonly name: typeof APIERROR;
  readonly status: number | undefined;
  readonly code: string | undefined;
  readonly requestId: string | null | undefined;
  readonly body: unknown;
  readonly retryAfterMs: number | null;

  constructor(message: string, details: ApiErrorDetails = {}) {
    super(message);
    this.name = APIERROR;
    this.status = details.status;
    this.code = details.code;
    this.requestId = details.requestId;
    this.body = details.body;
    // retryAfterMs is captured at the apiRequest boundary (where the
    // Response object is in scope). null when the header was absent or
    // unparseable. apiRequestWithRetry reads this directly so the
    // 429/Retry-After floor doesn't depend on body shape.
    this.retryAfterMs = details.retryAfterMs ?? null;
  }
}

function parseRetryAfterHeaderValue(headerVal: string | null | undefined): number | null {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n) && n >= 0) return n * MS_PER_SECOND;
  // HTTP-date form is rare for our APIs; fall back to ignoring.
  return null;
}

export type FetchImpl = (url: string, init?: RequestInit) => Promise<Response>;

export interface ApiRequestOptions {
  method?: string;
  headers?: Record<string, string>;
  body?: string;
  timeoutMs?: number;
  fetchImpl?: FetchImpl | undefined;
}

export async function apiRequest(url: string, options: ApiRequestOptions = {}): Promise<unknown> {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const fetchImpl: FetchImpl | undefined = options.fetchImpl ?? (globalThis.fetch as FetchImpl | undefined);
  if (typeof fetchImpl !== "function") {
    throw new ApiError("fetch is unavailable", { code: "NO_FETCH" });
  }

  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);

  try {
    const init: RequestInit = {
      method: options.method ?? "GET",
      headers: options.headers ?? {},
      signal: ctrl.signal,
    };
    if (options.body !== undefined) init.body = options.body;
    const res = await fetchImpl(url, init);

    const text = await res.text();
    let json: unknown = null;
    if (text.length > 0) {
      try {
        json = JSON.parse(text);
      } catch {
        json = null;
      }
    }

    if (!res.ok) {
      const problem = readProblemDetails(json);
      const errorCode = problem.code ?? `HTTP_${res.status}`;
      const requestId = problem.requestId ?? null;
      const message = problem.message ?? res.statusText ?? "request failed";
      // Capture Retry-After at the boundary where res.headers is still
      // in scope; ApiError.body intentionally carries only the parsed
      // payload, so the header lives on a dedicated field.
      const retryAfterMs = parseRetryAfterHeaderValue(res.headers?.get?.("retry-after"));
      throw new ApiError(message, {
        status: res.status,
        code: errorCode,
        requestId,
        body: json ?? text,
        retryAfterMs,
      });
    }

    return json ?? {};
  } catch (err) {
    if (err !== null && typeof err === TYPE_OBJECT && (err as { name?: unknown }).name === "AbortError") {
      throw new ApiError(`request timed out after ${timeoutMs}ms`, {
        status: 408,
        code: "TIMEOUT",
      });
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

const readString = (value: unknown): string | undefined =>
  typeof value === TYPE_STRING ? value as string : undefined;

const readStringArray = (value: unknown): ReadonlyArray<string> | undefined =>
  Array.isArray(value) && value.every((item) => typeof item === TYPE_STRING)
    ? value
    : undefined;

export function readProblemDetails(value: unknown): ProblemDetails {
  if (value === null || typeof value !== TYPE_OBJECT) return {};
  const body = value as Record<string, unknown>;
  const nested = body.error !== null && typeof body.error === TYPE_OBJECT
    ? body.error as Record<string, unknown>
    : null;
  return {
    code: readString(nested?.code) ?? readString(body.error_code),
    message: readString(nested?.message) ?? readString(body.detail) ?? readString(body.title),
    requestId: readString(nested?.request_id) ?? readString(body.request_id),
    missingSecrets: readStringArray(body.missing_secrets),
  };
}

// POST-based SSE streaming consumer lives in stream-fetch.ts (the
// mirror module to lib/sse.ts which owns the GET transport). Re-export
// is intentionally NOT added — callers import directly from
// stream-fetch.ts so the dependency direction stays honest.

export interface AuthCredentials {
  token?: string | null | undefined;
}

export function authHeaders(auth?: AuthCredentials | null): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (auth?.token) {
    headers.Authorization = `Bearer ${auth.token}`;
  }

  return headers;
}
