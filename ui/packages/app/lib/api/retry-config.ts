import { ApiError } from "./errors";
import { TRANSIENT_STATUSES } from "./retry-classify";

/**
 * The retry policy's options: what a caller may set, what each defaults to,
 * and the one place they are validated. `retry.ts` runs the schedule these
 * describe; `client.ts` passes them through unchanged.
 */

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
// No retry, and no sleep before one, begins past this point after the first
// attempt started; a render's worst case is this plus one attempt's own
// timeout. Next serialises a tab's Server Actions, so this is also how long an
// approve or a steer queued behind a slow read can be held.
const DEFAULT_DEADLINE_MS = 20_000;
// The longest wait an intermediary can ask for and still be obeyed. Above it
// the 429 is surfaced at once rather than holding a render for its whole ask.
const DEFAULT_RETRY_AFTER_CAP_MS = 10_000;
const MAX_ATTEMPTS_HARD_CAP = 10;
const RETRY_CODE_CONFIG_INVALID = "CONFIG_INVALID";

/**
 * What `onRetry` names as the reason. `"fatal"` is never observed there: the
 * gate refuses a fatal failure before the schedule runs.
 */
export type RetryReason = "timeout" | "425" | "429" | "5xx" | "network" | "fatal";

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
  /** The sleep the schedule chose before the next attempt. */
  delayMs: number;
};

export type SleepFn = (ms: number, signal?: AbortSignal) => Promise<void>;

export type RetryOptions<T = unknown> = {
  maxAttempts?: number;
  baseDelayMs?: number;
  capDelayMs?: number;
  /** No retry, and no sleep before one, begins once this much time has passed since the first attempt started. */
  deadlineMs?: number;
  /** A `Retry-After` above this fails the request at once instead of sleeping. */
  retryAfterCapMs?: number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
  /** Reads the status a successful attempt answered with, for the terminal `onAttempt`. */
  statusOf?: (value: T) => number | undefined;
  /**
   * Test seam: replaces the schedule's clock so tests don't tick real time.
   * A sleep in progress is abandoned, not awaited, when `signal` aborts.
   */
  sleepImpl?: SleepFn;
  /** Test seam: replaces `Math.random`, the draw full jitter scales a delay by. */
  randomFn?: () => number;
  /**
   * The caller's cancellation. Once it is aborted the schedule is interrupted
   * wherever it is: no further attempt is made and a sleep in progress ends,
   * so a page the operator has left never holds a server render for its
   * retry schedule.
   */
  signal?: AbortSignal;
  /**
   * What the policy throws when `signal` aborts — the caller's own cancel
   * class, so a cancel never surfaces as the transient error that preceded
   * it. Defaults to the signal's abort reason.
   */
  cancelled?: () => Error;
};

export type ResolvedRetry<T = unknown> = {
  maxAttempts: number;
  baseDelayMs: number;
  capDelayMs: number;
  deadlineMs: number;
  retryAfterCapMs: number;
  randomFn: () => number;
  sleep: SleepFn | undefined;
  onAttempt: ((info: AttemptInfo) => void) | undefined;
  onRetry: ((info: RetryInfo) => void) | undefined;
  statusOf: ((value: T) => number | undefined) | undefined;
  signal: AbortSignal | undefined;
  cancelled: (() => Error) | undefined;
};

function isNoRetryEnv(): boolean {
  // `process.env` is defined in every runtime this ships to (Node on the
  // server, the webpack/Edge shim in the browser); a non-public var simply
  // reads back undefined off-server, so no `typeof process` guard is needed.
  const v = process.env.AGENTSFLEET_NO_RETRY;
  return v === "1" || v === "true";
}

function configInvalid(detail: string): ApiError {
  return new ApiError(`retry.${detail}`, 0, RETRY_CODE_CONFIG_INVALID);
}

// A NaN or negative delay would become a hot loop or a schedule that never
// sleeps; refused here, once, before any attempt runs.
function requireDelay(name: string, value: number): number {
  if (!Number.isFinite(value) || value < 0) {
    throw configInvalid(`${name} must be a finite, non-negative number of milliseconds`);
  }
  return value;
}

export function resolveRetryConfig<T>(options: RetryOptions<T>): ResolvedRetry<T> {
  const maxAttemptsRaw = options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  if (!Number.isInteger(maxAttemptsRaw) || maxAttemptsRaw < 1 || maxAttemptsRaw > MAX_ATTEMPTS_HARD_CAP) {
    throw configInvalid(`maxAttempts must be an integer in 1..${MAX_ATTEMPTS_HARD_CAP}`);
  }
  return {
    maxAttempts: isNoRetryEnv() ? 1 : maxAttemptsRaw,
    baseDelayMs: requireDelay("baseDelayMs", options.baseDelayMs ?? DEFAULT_BASE_DELAY_MS),
    capDelayMs: requireDelay("capDelayMs", options.capDelayMs ?? DEFAULT_CAP_DELAY_MS),
    deadlineMs: requireDelay("deadlineMs", options.deadlineMs ?? DEFAULT_DEADLINE_MS),
    retryAfterCapMs: requireDelay("retryAfterCapMs", options.retryAfterCapMs ?? DEFAULT_RETRY_AFTER_CAP_MS),
    randomFn: options.randomFn ?? Math.random,
    sleep: options.sleepImpl,
    onAttempt: options.onAttempt,
    onRetry: options.onRetry,
    statusOf: options.statusOf,
    signal: options.signal,
    cancelled: options.cancelled,
  };
}

export const RETRY_DEFAULTS = {
  maxAttempts: DEFAULT_MAX_ATTEMPTS,
  baseDelayMs: DEFAULT_BASE_DELAY_MS,
  capDelayMs: DEFAULT_CAP_DELAY_MS,
  deadlineMs: DEFAULT_DEADLINE_MS,
  retryAfterCapMs: DEFAULT_RETRY_AFTER_CAP_MS,
  hardCap: MAX_ATTEMPTS_HARD_CAP,
  statuses: TRANSIENT_STATUSES,
};
