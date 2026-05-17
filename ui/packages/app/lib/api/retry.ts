import { ApiError } from "./errors";
import { request } from "./client";

/**
 * HTTP retry wrapper mirroring `zombiectl/src/lib/http.js`'s
 * `apiRequestWithRetry`. Same retryable-status set, same backoff
 * math, same Retry-After honoring, same `onAttempt`/`onRetry` hook
 * surface — so dashboard + CLI behaviour stays consistent for the
 * operator. Bounds + defaults are pinned identical to keep one
 * mental model.
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
  | "network"
  | "server_marked_retryable";

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
    if (err.status && RETRYABLE_STATUSES.has(err.status)) {
      if (err.status === 429) return "429";
      return "5xx";
    }
    if (typeof err.code === "string" && /^UZ-[A-Z0-9]+-RETRY/.test(err.code)) {
      return "server_marked_retryable";
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
  if (typeof process === "undefined") return false;
  const v = process.env?.ZOMBIE_NO_RETRY;
  return v === "1" || v === "true";
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
  const maxAttemptsRaw = options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  const baseDelayMs = options.baseDelayMs ?? DEFAULT_BASE_DELAY_MS;
  const capDelayMs = options.capDelayMs ?? DEFAULT_CAP_DELAY_MS;
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
  const maxAttempts = isNoRetryEnv() ? 1 : maxAttemptsRaw;
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
      const result = await request<T>(path, init, token);
      const durationMs = Date.now() - startedAt;
      if (onAttempt) {
        onAttempt({
          attempt,
          status: 200,
          durationMs,
          retryCount: attempt - 1,
          terminal: true,
        });
      }
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const reason = classifyRetryable(err);
      const status = err instanceof ApiError ? err.status : undefined;
      const willRetry = reason !== null && attempt < maxAttempts;
      if (willRetry) {
        if (onRetry) {
          onRetry({ attempt, status, durationMs, reason: reason as RetryReason });
        }
        const retryAfterMs = err instanceof ApiError ? err.retryAfterMs : null;
        const delay = backoffDelay({
          attempt,
          baseDelayMs,
          capDelayMs,
          retryAfterMs,
          randomFn,
        });
        await sleep(delay);
        lastErr = err;
        continue;
      }
      if (onAttempt) {
        onAttempt({
          attempt,
          status,
          durationMs,
          retryCount: attempt - 1,
          terminal: true,
        });
      }
      throw err;
    }
  }
  throw lastErr ?? new ApiError("requestWithRetry exhausted", 0, "INTERNAL");
}

export const RETRY_DEFAULTS = {
  maxAttempts: DEFAULT_MAX_ATTEMPTS,
  baseDelayMs: DEFAULT_BASE_DELAY_MS,
  capDelayMs: DEFAULT_CAP_DELAY_MS,
  hardCap: MAX_ATTEMPTS_HARD_CAP,
  statuses: Array.from(RETRYABLE_STATUSES) as readonly number[],
};
