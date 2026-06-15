import { ApiError } from "./errors";
import { request } from "./client";

/**
 * HTTP retry wrapper mirroring `cli/src/lib/http-retry.ts`'s
 * `apiRequestWithRetry`. Same retryable-status set, same backoff
 * math, same Retry-After honoring, same server-5xx idempotency gate,
 * same `onAttempt`/`onRetry` hook surface — so dashboard + CLI
 * behaviour stays consistent for the operator. Bounds + defaults are
 * pinned identical to keep one mental model.
 */

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;

const RETRYABLE_STATUSES = new Set<number>([408, 425, 429, 502, 503, 504]);

export type RetryReason =
  | "timeout"
  | "429"
  | "5xx"
  | "network";

export type AttemptInfo = {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  retryCount: number;
  terminal: boolean;
};

export type RetryInfo = {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  reason: RetryReason;
};

export type RetryOptions = {
  maxAttempts?: number;
  baseDelayMs?: number;
  capDelayMs?: number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
  /** Test seam: replaces wall-clock sleep so tests don't tick real time. */
  sleepImpl?: (ms: number) => Promise<void>;
  /** Test seam: replaces `Math.random` so jitter is deterministic. */
  randomFn?: () => number;
};

export function classifyRetryable(err: unknown): RetryReason | null {
  if (err instanceof ApiError) {
    if (err.code === "TIMEOUT") return "timeout";
    if (RETRYABLE_STATUSES.has(err.status)) {
      if (err.status === 429) return "429";
      return "5xx";
    }
    return null;
  }
  if (
    err instanceof TypeError &&
    typeof err.message === "string" &&
    err.message.toLowerCase().includes("fetch failed")
  ) {
    return "network";
  }
  // Node-shaped network errors (server-side rendering path).
  const maybe = err as { code?: string } | null;
  if (maybe && typeof maybe.code === "string") {
    if (
      maybe.code === "ECONNRESET" ||
      maybe.code === "ETIMEDOUT" ||
      maybe.code === "ENOTFOUND"
    ) {
      return "network";
    }
  }
  return null;
}

/**
 * HTTP methods safe to replay. A genuine server 5xx (>=500) may have been
 * processed upstream before the gateway error surfaced, so replaying a
 * non-idempotent method (POST/PATCH) risks a duplicate mutation. Mirrors the
 * Supabase CLI's `isRetryableResponse` idempotency gate.
 */
export function isIdempotentMethod(method: string): boolean {
  const m = method.toUpperCase();
  return m === "GET" || m === "PUT" || m === "DELETE" || m === "HEAD";
}

export function backoffDelay({
  attempt,
  baseDelayMs,
  capDelayMs,
  retryAfterMs,
  randomFn,
}: {
  attempt: number;
  baseDelayMs: number;
  capDelayMs: number;
  retryAfterMs: number | null;
  randomFn: () => number;
}): number {
  if (typeof retryAfterMs === "number" && retryAfterMs > 0) {
    // Server-supplied floor. +0..20% jitter so a herd of clients
    // doesn't synchronize their next attempt.
    return retryAfterMs + retryAfterMs * 0.2 * randomFn();
  }
  const base = Math.min(baseDelayMs * Math.pow(2, attempt - 1), capDelayMs);
  // ±20% jitter centered on the base.
  const jitter = base * 0.2 * (randomFn() * 2 - 1);
  return Math.max(0, base + jitter);
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isNoRetryEnv(): boolean {
  // `process.env` is defined in every runtime this ships to (Node on the
  // server, the webpack/Edge shim in the browser); a non-public var simply
  // reads back undefined off-server, so no `typeof process` guard is needed.
  const v = process.env.AGENTSFLEET_NO_RETRY;
  return v === "1" || v === "true";
}

type ResolvedRetry = {
  maxAttempts: number;
  baseDelayMs: number;
  capDelayMs: number;
  sleep: (ms: number) => Promise<void>;
  randomFn: () => number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
};

function resolveRetryConfig(options: RetryOptions): ResolvedRetry {
  const maxAttemptsRaw = options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  if (
    !Number.isInteger(maxAttemptsRaw) ||
    maxAttemptsRaw < 1 ||
    maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP
  ) {
    throw new ApiError(
      `retry.maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`,
      0,
      "CONFIG_INVALID",
    );
  }
  return {
    maxAttempts: isNoRetryEnv() ? 1 : maxAttemptsRaw,
    baseDelayMs: options.baseDelayMs ?? DEFAULT_BASE_DELAY_MS,
    capDelayMs: options.capDelayMs ?? DEFAULT_CAP_DELAY_MS,
    sleep: options.sleepImpl ?? defaultSleep,
    randomFn: options.randomFn ?? Math.random,
    onAttempt: options.onAttempt,
    onRetry: options.onRetry,
  };
}

/** Terminal `onAttempt` telemetry for a finished attempt — success (status
 * 200) or the failing status on the last try. */
function emitTerminalAttempt(
  onAttempt: ((info: AttemptInfo) => void) | undefined,
  attempt: number,
  status: number | undefined,
  durationMs: number,
): void {
  if (onAttempt) {
    onAttempt({ attempt, status, durationMs, retryCount: attempt - 1, terminal: true });
  }
}

type AttemptContext = {
  attempt: number;
  status: number | undefined;
  durationMs: number;
  method: string;
};

/** Decides whether a failed attempt retries. Returns the backoff delay (and
 * fires `onRetry`) when it should, else null. The server-5xx idempotency gate
 * blocks replay of non-idempotent methods. */
function planRetry(
  err: unknown,
  cfg: ResolvedRetry,
  ctx: AttemptContext,
): { delayMs: number } | null {
  const reason = classifyRetryable(err);
  const isServer5xx = ctx.status !== undefined && ctx.status >= 500;
  const unsafeReplay = reason === "5xx" && isServer5xx && !isIdempotentMethod(ctx.method);
  if (reason === null || unsafeReplay || ctx.attempt >= cfg.maxAttempts) return null;
  if (cfg.onRetry) {
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

/**
 * Wraps `request<T>` with the CLI-parity retry policy. On success the
 * unwrapped body is returned exactly as `request<T>` returns it. On
 * non-retryable failure (or after `maxAttempts` exhausted) the last
 * `ApiError` is re-thrown.
 */
export async function requestWithRetry<T>(
  path: string,
  init: RequestInit,
  token: string,
  options: RetryOptions = {},
): Promise<T> {
  const cfg = resolveRetryConfig(options);
  const method = init.method ?? "GET";
  // `planRetry` owns the ceiling (`attempt < maxAttempts`); on the final
  // attempt it returns null, so the loop always exits via return (success)
  // or throw (failure) — no normal fall-through after the loop.
  let attempt = 0;
  for (;;) {
    attempt += 1;
    const startedAt = Date.now();
    try {
      const result = await request<T>(path, init, token);
      emitTerminalAttempt(cfg.onAttempt, attempt, 200, Date.now() - startedAt);
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const status = err instanceof ApiError ? err.status : undefined;
      const step = planRetry(err, cfg, { attempt, status, durationMs, method });
      if (step) {
        await cfg.sleep(step.delayMs);
        continue;
      }
      emitTerminalAttempt(cfg.onAttempt, attempt, status, durationMs);
      throw err;
    }
  }
}

export const RETRY_DEFAULTS = {
  maxAttempts: DEFAULT_MAX_ATTEMPTS,
  baseDelayMs: DEFAULT_BASE_DELAY_MS,
  capDelayMs: DEFAULT_CAP_DELAY_MS,
  hardCap: MAX_ATTEMPTS_HARD_CAP,
  statuses: Array.from(RETRYABLE_STATUSES) as readonly number[],
};
