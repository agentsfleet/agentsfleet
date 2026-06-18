// HTTP retry-with-backoff layer over the core `apiRequest` transport.
// Owns retry classification, exponential backoff + jitter, Retry-After
// honoring, the AGENTSFLEET_NO_RETRY escape hatch, and the server-5xx
// idempotency gate. Split out of http.ts so transport and retry concerns
// stay separable and each module stays under the line cap.

import { ApiError, apiRequest, type ApiRequestOptions } from "./http.ts";

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;
const RETRYABLE_STATUSES = new Set<number>([408, 425, 429, 502, 503, 504]);
const RETRY_REASON_429 = "429" as const;
const RETRY_REASON_5XX = "5xx" as const;
const HTTP_METHOD_GET = "GET" as const;
const RETRY_REASON_NETWORK = "network" as const;
const TYPE_OBJECT = "object" as const;
const STATUS_TIMEOUT = "timeout" as const;

// Reasons surfaced on the `onRetry` callback so the analytics layer
// can attribute the retry to a concrete failure class.
export type RetryReason =
  | typeof STATUS_TIMEOUT
  | typeof RETRY_REASON_429
  | typeof RETRY_REASON_5XX
  | typeof RETRY_REASON_NETWORK;

function hasRetryOptOut(body: unknown): boolean {
  if (body === null || typeof body !== TYPE_OBJECT) return false;
  const errField = (body as { error?: unknown }).error;
  if (errField === null || typeof errField !== TYPE_OBJECT) return false;
  return (errField as { retry_after_seconds?: unknown }).retry_after_seconds === 0;
}

function classifyRetryable(err: unknown): RetryReason | null {
  if (err instanceof ApiError) {
    if (err.code === "TIMEOUT") return STATUS_TIMEOUT;
    if (err.status !== undefined && RETRYABLE_STATUSES.has(err.status)) {
      // Server can opt out of retries by sending Retry-After: 0; we
      // surface that on the body so the wrapper can honor it.
      if (hasRetryOptOut(err.body)) return null;
      if (err.status === 429) return RETRY_REASON_429;
      return RETRY_REASON_5XX;
    }
    return null;
  }
  if (
    err instanceof TypeError
    && typeof err.message === "string"
    && err.message.toLowerCase().includes("fetch failed")
  ) {
    return RETRY_REASON_NETWORK;
  }
  if (err !== null && typeof err === TYPE_OBJECT) {
    const code = (err as { code?: unknown }).code;
    if (code === "ECONNRESET" || code === "ETIMEDOUT" || code === "ENOTFOUND") {
      return RETRY_REASON_NETWORK;
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

function noRetryEnv(env: NodeJS.ProcessEnv | undefined): boolean {
  const v = env?.AGENTSFLEET_NO_RETRY;
  return v === "1" || v === "true";
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

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

export interface ApiRequestWithRetryOptions extends ApiRequestOptions {
  // null/undefined → use apiRequestWithRetry's defaults.
  retry?: RetryConfig | null | undefined;
  env?: NodeJS.ProcessEnv;
  sleepImpl?: (ms: number) => Promise<void>;
  randomFn?: () => number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
}

/**
 * HTTP methods safe to replay. A genuine server 5xx (>=500) may have been
 * processed upstream before the gateway error surfaced, so replaying a
 * non-idempotent method (POST/PATCH) risks a duplicate mutation. Mirrors the
 * Supabase CLI's `isRetryableResponse` idempotency gate.
 */
export function isIdempotentMethod(method: string): boolean {
  const m = method.toUpperCase();
  return m === HTTP_METHOD_GET || m === "PUT" || m === "DELETE" || m === "HEAD";
}

interface ResolvedRetryRuntime {
  maxAttempts: number;
  baseDelayMs: number;
  capDelayMs: number;
  sleep: (ms: number) => Promise<void>;
  randomFn: () => number;
  onAttempt: ((info: AttemptInfo) => void) | undefined;
  onRetry: ((info: RetryInfo) => void) | undefined;
  method: string;
}

function resolveRetryRuntime(options: ApiRequestWithRetryOptions): ResolvedRetryRuntime {
  const retryCfg = options.retry ?? {};
  const maxAttemptsRaw = retryCfg.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  // Bounds enforcement per Invariant #4: maxAttempts ∈ [1, 10]. Out-of-range
  // is misconfiguration, not a runtime decision.
  if (!Number.isInteger(maxAttemptsRaw) || maxAttemptsRaw < 1 || maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP) {
    throw new ApiError(`retry.maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`, {
      code: "CONFIG_INVALID",
    });
  }
  const env = options.env ?? (typeof process !== "undefined" ? process.env : undefined);
  return {
    maxAttempts: noRetryEnv(env) ? 1 : maxAttemptsRaw,
    baseDelayMs: retryCfg.baseDelayMs ?? DEFAULT_BASE_DELAY_MS,
    capDelayMs: retryCfg.capDelayMs ?? DEFAULT_CAP_DELAY_MS,
    sleep: options.sleepImpl ?? defaultSleep,
    randomFn: options.randomFn ?? Math.random,
    onAttempt: options.onAttempt,
    onRetry: options.onRetry,
    method: options.method ?? HTTP_METHOD_GET,
  };
}

function emitTerminalAttempt(
  onAttempt: ((info: AttemptInfo) => void) | undefined,
  attempt: number,
  status: number | undefined,
  durationMs: number,
): void {
  if (onAttempt !== undefined) {
    onAttempt({ attempt, status, durationMs, retryCount: attempt - 1, terminal: true });
  }
}

interface AttemptContext {
  attempt: number;
  status: number | undefined;
  durationMs: number;
}

// Decides whether a failed attempt retries. Returns the backoff delay (and
// fires onRetry) when it should, else null. The server-5xx idempotency gate
// blocks replay of non-idempotent methods.
function planRetry(
  err: unknown,
  cfg: ResolvedRetryRuntime,
  ctx: AttemptContext,
): { delayMs: number } | null {
  const reason = classifyRetryable(err);
  const isServer5xx = ctx.status !== undefined && ctx.status >= 500;
  const unsafeReplay = reason === "5xx" && isServer5xx && !isIdempotentMethod(cfg.method);
  if (reason === null || unsafeReplay || ctx.attempt >= cfg.maxAttempts) return null;
  if (cfg.onRetry !== undefined) {
    cfg.onRetry({ attempt: ctx.attempt, status: ctx.status, durationMs: ctx.durationMs, reason });
  }
  const retryAfterMs = err instanceof ApiError ? err.retryAfterMs : null;
  const delayMs = backoffDelay({
    attempt: ctx.attempt,
    baseDelayMs: cfg.baseDelayMs,
    capDelayMs: cfg.capDelayMs,
    retryAfterMs,
    randomFn: cfg.randomFn,
  });
  return { delayMs };
}

export async function apiRequestWithRetry(
  url: string,
  options: ApiRequestWithRetryOptions = {},
): Promise<unknown> {
  const cfg = resolveRetryRuntime(options);
  async function runAttempt(attempt: number): Promise<unknown> {
    const startedAt = Date.now();
    try {
      const result = await apiRequest(url, options);
      emitTerminalAttempt(cfg.onAttempt, attempt, 200, Date.now() - startedAt);
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const status = err instanceof ApiError ? err.status : undefined;
      const step = planRetry(err, cfg, { attempt, status, durationMs });
      if (step) {
        await cfg.sleep(step.delayMs);
        return runAttempt(attempt + 1);
      }
      emitTerminalAttempt(cfg.onAttempt, attempt, status, durationMs);
      throw err;
    }
  }
  return runAttempt(1);
}
