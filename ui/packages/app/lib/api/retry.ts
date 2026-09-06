import { ApiError } from "./errors";
import { backoffDelay, defaultSleep, sleepUnlessAborted } from "./retry-backoff";

// Part of this module's public surface; it lives beside the sleep it pairs with.
export { backoffDelay };

/**
 * The retry policy mirroring `cli/src/lib/http-retry.ts`'s
 * `apiRequestWithRetry`. Same retryable-status set, same backoff
 * math, same Retry-After honoring, same `onAttempt`/`onRetry` hook
 * surface — so dashboard + CLI behaviour stays consistent for the
 * operator. Bounds + defaults are pinned identical to keep one mental
 * model. One deliberate difference: the dashboard's idempotency gate
 * also refuses to replay a non-idempotent method after a client-side
 * timeout, because every dashboard request carries one by default
 * (`client.ts`) and a steer POST the server did process must not become
 * two events. The CLI's gate (`cli/src/lib/http-retry.ts`) does not
 * close that case yet; the shared fixture table that proves both
 * runtimes from one source is the follow-up that reconciles them.
 *
 * Policy only: this module never imports the transport. `client.ts` owns the
 * single attempt and wraps it with `runWithRetry`, so the dependency points
 * one way and neither side can retry the other's retry.
 */

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_BASE_DELAY_MS = 250;
const DEFAULT_CAP_DELAY_MS = 2000;
const MAX_ATTEMPTS_HARD_CAP = 10;

/**
 * The status a client-side timeout is reported under. 408 is the closest
 * standard meaning (the request did not complete in time) and sits in the
 * transient set below, so the transport's timeout and a server's 408 are one
 * class to the classifier. Declared here, beside the set that reads it, so the
 * transport imports the number instead of re-spelling it.
 */
export const HTTP_STATUS_REQUEST_TIMEOUT = 408;

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

const RETRYABLE_STATUSES = new Set<number>([HTTP_STATUS_REQUEST_TIMEOUT, 425, 429, 502, 503, 504]);

/**
 * The HTTP methods the policy and the transport both name: the idempotency
 * gate below reads them, and `client.ts` builds its default retry set from
 * them. One declaration so the two sets can never disagree on a spelling.
 */
export const HTTP_METHOD = {
  GET: "GET",
  HEAD: "HEAD",
  PUT: "PUT",
  DELETE: "DELETE",
} as const;

/**
 * The `ApiError.code` a client-side request timeout carries — the retry
 * layer's own class, distinct from any `UZ-` wire code. The classifier and the
 * transport both read it, so it is declared once here.
 */
export const RETRY_CODE_TIMEOUT = "TIMEOUT";
const RETRY_CODE_CONFIG_INVALID = "CONFIG_INVALID";

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
  /**
   * Test seam: replaces wall-clock sleep so tests don't tick real time. A
   * seam that ignores `signal` still ends early — the loop races it against
   * the abort — but only the default sleep can also clear its timer.
   */
  sleepImpl?: (ms: number, signal?: AbortSignal) => Promise<void>;
  /** Test seam: replaces `Math.random` so jitter is deterministic. */
  randomFn?: () => number;
  /**
   * The caller's cancellation. Once it is aborted the loop stops: no further
   * attempt is made and a backoff already in progress ends early, so a page
   * the operator has left never holds a server render for its retry schedule.
   */
  signal?: AbortSignal;
  /**
   * What the loop throws when `signal` aborts during a backoff — the caller's
   * own cancel class, so a cancel never surfaces as the transient error that
   * preceded it. Defaults to the signal's abort reason.
   */
  cancelled?: () => Error;
};

export function classifyRetryable(err: unknown): RetryReason | null {
  if (err instanceof ApiError) {
    if (err.code === RETRY_CODE_TIMEOUT) return "timeout";
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
 * processed upstream before the gateway error surfaced, and a request that
 * timed out client-side may equally have reached the server, so replaying a
 * non-idempotent method (POST/PATCH) on either risks a duplicate mutation.
 * Mirrors the Supabase CLI's `isRetryableResponse` idempotency gate.
 */
export function isIdempotentMethod(method: string): boolean {
  const m = method.toUpperCase();
  return (
    m === HTTP_METHOD.GET || m === HTTP_METHOD.PUT || m === HTTP_METHOD.DELETE || m === HTTP_METHOD.HEAD
  );
}

// The error a cancel during backoff surfaces as: the caller's own class when
// it named one, else the signal's reason, else the error already in hand.
function cancelledError(cfg: ResolvedRetry, lastErr: unknown): unknown {
  if (cfg.cancelled) return cfg.cancelled();
  const reason: unknown = cfg.signal?.reason;
  return reason instanceof Error ? reason : lastErr;
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
  sleep: (ms: number, signal?: AbortSignal) => Promise<void>;
  randomFn: () => number;
  onAttempt?: (info: AttemptInfo) => void;
  onRetry?: (info: RetryInfo) => void;
  signal?: AbortSignal;
  cancelled?: () => Error;
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
      RETRY_CODE_CONFIG_INVALID,
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
    signal: options.signal,
    cancelled: options.cancelled,
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

/**
 * True when a failed attempt may already have been processed by the server,
 * so a non-idempotent method must not be sent again: a genuine server 5xx
 * (the gateway answered after the handler may have run) and a client-side
 * timeout (the request was on the wire; the answer is what never came). A
 * server 408/425/429 is the server saying it did NOT process the request, so
 * those stay replayable for every method.
 */
function mayHaveBeenProcessed(reason: RetryReason, status: number | undefined): boolean {
  if (reason === "timeout") return true;
  return reason === "5xx" && status !== undefined && status >= HTTP_STATUS_SERVER_ERROR_FLOOR;
}

/** Decides whether a failed attempt retries. Returns the backoff delay (and
 * fires `onRetry`) when it should, else null. The idempotency gate blocks
 * replay of a non-idempotent method whenever the server may have processed
 * the attempt; a caller that has already cancelled gets no further attempt,
 * whatever the failure class. */
function planRetry(
  err: unknown,
  cfg: ResolvedRetry,
  ctx: AttemptContext,
): { delayMs: number } | null {
  if (cfg.signal?.aborted) return null;
  const reason = classifyRetryable(err);
  if (reason === null) return null;
  const unsafeReplay = mayHaveBeenProcessed(reason, ctx.status) && !isIdempotentMethod(ctx.method);
  if (unsafeReplay || ctx.attempt >= cfg.maxAttempts) return null;
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
 * Runs one attempt under the policy. `method` decides the replay gate — a
 * POST or PATCH that may have reached the server (a 5xx, a client timeout) is
 * never replayed. On success the attempt's value is returned as is. On a
 * non-retryable failure (or after `maxAttempts` exhausted) the last error is
 * re-thrown.
 */
export async function runWithRetry<T>(
  attempt: () => Promise<T>,
  method: string,
  options: RetryOptions = {},
): Promise<T> {
  const cfg = resolveRetryConfig(options);
  // `planRetry` owns the ceiling (`attempt < maxAttempts`); on the final
  // attempt it returns null, so the loop always exits via return (success)
  // or throw (failure) — no normal fall-through after the loop.
  let attemptNumber = 0;
  for (;;) {
    attemptNumber += 1;
    const startedAt = Date.now();
    try {
      const result = await attempt();
      emitTerminalAttempt(cfg.onAttempt, attemptNumber, 200, Date.now() - startedAt);
      return result;
    } catch (err) {
      const durationMs = Date.now() - startedAt;
      const status = err instanceof ApiError ? err.status : undefined;
      const step = planRetry(err, cfg, { attempt: attemptNumber, status, durationMs, method });
      if (step) {
        await sleepUnlessAborted(cfg.sleep, step.delayMs, cfg.signal);
        if (!cfg.signal?.aborted) continue;
        // A cancel that landed during the backoff ends the loop here, as the
        // caller's cancel — the transient error in hand is not what happened,
        // and the next attempt would only fail against an aborted signal.
        emitTerminalAttempt(cfg.onAttempt, attemptNumber, status, durationMs);
        throw cancelledError(cfg, err);
      }
      emitTerminalAttempt(cfg.onAttempt, attemptNumber, status, durationMs);
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
