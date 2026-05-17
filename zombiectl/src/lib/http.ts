// HTTP transport: fetch wrapper, retry-with-backoff layer, JSON envelope
// unwrap, AbortController-backed timeout, POST-mode SSE streaming. Every
// non-OK response surfaces as ApiError; the wrapper's caller never sees
// a raw Response.

const DEFAULT_TIMEOUT_MS = 15000;
const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;
const RETRYABLE_STATUSES = new Set<number>([408, 425, 429, 502, 503, 504]);

// Reasons surfaced on the `onRetry` callback so the analytics layer
// can attribute the retry to a concrete failure class.
export type RetryReason =
  | "timeout"
  | "429"
  | "5xx"
  | "server_marked_retryable"
  | "network";

export interface ApiErrorDetails {
  status?: number;
  code?: string;
  requestId?: string | null;
  body?: unknown;
  retryAfterMs?: number | null;
}

export class ApiError extends Error {
  override readonly name: "ApiError";
  readonly status: number | undefined;
  readonly code: string | undefined;
  readonly requestId: string | null | undefined;
  readonly body: unknown;
  readonly retryAfterMs: number | null;

  constructor(message: string, details: ApiErrorDetails = {}) {
    super(message);
    this.name = "ApiError";
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

function hasRetryOptOut(body: unknown): boolean {
  if (body === null || typeof body !== "object") return false;
  const errField = (body as { error?: unknown }).error;
  if (errField === null || typeof errField !== "object") return false;
  return (errField as { retry_after_seconds?: unknown }).retry_after_seconds === 0;
}

function classifyRetryable(err: unknown): RetryReason | null {
  if (err instanceof ApiError) {
    if (err.code === "TIMEOUT") return "timeout";
    if (err.status !== undefined && RETRYABLE_STATUSES.has(err.status)) {
      // Server can opt out of retries by sending Retry-After: 0; we
      // surface that on the body so the wrapper can honor it.
      if (hasRetryOptOut(err.body)) return null;
      if (err.status === 429) return "429";
      return "5xx";
    }
    if (typeof err.code === "string" && /^UZ-[A-Z0-9]+-RETRY/.test(err.code)) {
      return "server_marked_retryable";
    }
    return null;
  }
  if (
    err instanceof TypeError
    && typeof err.message === "string"
    && err.message.toLowerCase().includes("fetch failed")
  ) {
    return "network";
  }
  if (err !== null && typeof err === "object") {
    const code = (err as { code?: unknown }).code;
    if (code === "ECONNRESET" || code === "ETIMEDOUT" || code === "ENOTFOUND") {
      return "network";
    }
  }
  return null;
}

interface BackoffArgs {
  attempt: number;
  baseDelayMs: number;
  capDelayMs: number;
  retryAfterMs: number | null;
  randomFn: () => number;
}

function backoffDelay({ attempt, baseDelayMs, capDelayMs, retryAfterMs, randomFn }: BackoffArgs): number {
  if (typeof retryAfterMs === "number" && retryAfterMs > 0) {
    // Server-provided floor. Apply +0..20% jitter so a herd of clients
    // doesn't synchronize their next attempt.
    return retryAfterMs + retryAfterMs * 0.2 * randomFn();
  }
  const base = Math.min(baseDelayMs * Math.pow(2, attempt - 1), capDelayMs);
  // ±20% jitter centered on the base.
  const jitter = base * 0.2 * (randomFn() * 2 - 1);
  return Math.max(0, base + jitter);
}

function parseRetryAfterHeaderValue(headerVal: string | null | undefined): number | null {
  if (!headerVal) return null;
  const n = Number(headerVal);
  if (Number.isFinite(n) && n >= 0) return n * 1000;
  // HTTP-date form is rare for our APIs; fall back to ignoring.
  return null;
}

function noRetryEnv(env: NodeJS.ProcessEnv | undefined): boolean {
  const v = env?.ZOMBIE_NO_RETRY;
  return v === "1" || v === "true";
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export type FetchImpl = (url: string, init?: RequestInit) => Promise<Response>;

export interface RetryConfig {
  maxAttempts?: number;
  baseDelayMs?: number;
  capDelayMs?: number;
}

export interface AttemptInfo {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  retryCount: number;
  terminal: boolean;
}

export interface RetryInfo {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  reason: RetryReason;
}

export interface ApiRequestOptions {
  method?: string;
  headers?: Record<string, string>;
  body?: string;
  timeoutMs?: number;
  fetchImpl?: FetchImpl | undefined;
}

export interface ApiRequestWithRetryOptions extends ApiRequestOptions {
  // null/undefined → use apiRequestWithRetry's defaults.
  retry?: RetryConfig | null | undefined;
  env?: NodeJS.ProcessEnv;
  sleepImpl?: (ms: number) => Promise<void>;
  randomFn?: () => number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
}

export async function apiRequestWithRetry(
  url: string,
  options: ApiRequestWithRetryOptions = {},
): Promise<unknown> {
  const retryCfg = options.retry ?? {};
  const maxAttemptsRaw = retryCfg.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  const baseDelayMs = retryCfg.baseDelayMs ?? DEFAULT_BASE_DELAY_MS;
  const capDelayMs = retryCfg.capDelayMs ?? DEFAULT_CAP_DELAY_MS;
  // Bounds enforcement per spec Invariant #4: maxAttempts ∈ [1, 10].
  // Out-of-range is misconfiguration, not a runtime decision.
  if (!Number.isInteger(maxAttemptsRaw) || maxAttemptsRaw < 1 || maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP) {
    throw new ApiError(`retry.maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`, {
      code: "CONFIG_INVALID",
    });
  }
  const env = options.env ?? (typeof process !== "undefined" ? process.env : undefined);
  const maxAttempts = noRetryEnv(env) ? 1 : maxAttemptsRaw;
  const sleep = options.sleepImpl ?? defaultSleep;
  const randomFn = options.randomFn ?? Math.random;
  const onAttempt = options.onAttempt;
  const onRetry = options.onRetry;

  let attempt = 0;
  let lastErr: unknown = null;
  while (attempt < maxAttempts) {
    attempt += 1;
    const startedAt = Date.now();
    try {
      const result = await apiRequest(url, options);
      const durationMs = Date.now() - startedAt;
      if (typeof onAttempt === "function") {
        onAttempt({ attempt, status: 200, durationMs, retryCount: attempt - 1, terminal: true });
      }
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const reason = classifyRetryable(err);
      const status = err instanceof ApiError ? err.status : undefined;
      const willRetry = reason !== null && attempt < maxAttempts;
      if (willRetry) {
        if (typeof onRetry === "function") {
          onRetry({ attempt, status, durationMs, reason });
        }
        const retryAfterMs = err instanceof ApiError ? err.retryAfterMs : null;
        const delay = backoffDelay({ attempt, baseDelayMs, capDelayMs, retryAfterMs, randomFn });
        await sleep(delay);
        lastErr = err;
        continue;
      }
      if (typeof onAttempt === "function") {
        onAttempt({ attempt, status, durationMs, retryCount: attempt - 1, terminal: true });
      }
      throw err;
    }
  }
  // Defensive: while-loop should always either return or throw above.
  throw lastErr ?? new ApiError("apiRequestWithRetry exhausted without throw", { code: "INTERNAL" });
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
      const envelope = isErrorEnvelope(json) ? json : null;
      const errorCode = envelope?.error?.code ?? `HTTP_${res.status}`;
      const requestId = envelope?.error?.request_id ?? envelope?.request_id ?? null;
      const message = envelope?.error?.message ?? res.statusText ?? "request failed";
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
    if (err !== null && typeof err === "object" && (err as { name?: unknown }).name === "AbortError") {
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

interface ErrorEnvelope {
  error?: {
    code?: string;
    message?: string;
    request_id?: string | null;
  };
  request_id?: string | null;
}

function isErrorEnvelope(value: unknown): value is ErrorEnvelope {
  return value !== null && typeof value === "object";
}

// POST-based SSE streaming consumer lives in stream-fetch.ts (the
// mirror module to lib/sse.ts which owns the GET transport). Re-export
// is intentionally NOT added — callers import directly from
// stream-fetch.ts so the dependency direction stays honest.

export interface AuthCredentials {
  token?: string | null | undefined;
  apiKey?: string | null | undefined;
}

export function authHeaders(auth?: AuthCredentials | null): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  if (auth?.token) {
    headers.Authorization = `Bearer ${auth.token}`;
    return headers;
  }

  if (auth?.apiKey) {
    headers.Authorization = `Bearer ${auth.apiKey}`;
  }

  return headers;
}
