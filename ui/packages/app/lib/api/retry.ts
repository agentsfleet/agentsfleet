import * as Cause from "effect/Cause";
import * as Clock from "effect/Clock";
import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Exit from "effect/Exit";
import * as Schedule from "effect/Schedule";
import { ApiError } from "./errors";
import { FAILURE_KIND, PROVENANCE, classifyFailure, type ClassifiedFailure, type FailureKind } from "./retry-classify";
import {
  resolveRetryConfig,
  type AttemptInfo,
  type ResolvedRetry,
  type RetryOptions,
  type RetryReason,
} from "./retry-config";

// The options and their defaults are the policy's surface too. The status
// facts (`HTTP_STATUS_REQUEST_TIMEOUT`, `RETRY_CODE_TIMEOUT`,
// `isDefiniteRefusal`) live in `./errors`, which every importer reads directly:
// this module's dependency is server-only.
export { RETRY_DEFAULTS, type AttemptInfo, type RetryInfo, type RetryOptions, type RetryReason } from "./retry-config";

/**
 * The retry policy behind every dashboard request: a declared schedule with a
 * total deadline, not a loop. One attempt runs under `Effect.retry` and a
 * composed `Schedule` — exponential growth, capped, drawn with full jitter,
 * bounded by an attempt ceiling and by `deadlineMs`, and gated by what the
 * failure was and where it happened (`retry-classify.ts`). A `Retry-After`
 * within `retryAfterCapMs` is the sleep; one beyond it fails the request at
 * once. The caller's `signal` interrupts the schedule wherever it is.
 *
 * Policy only: this module never imports the transport. `client.ts` owns the
 * single attempt and wraps it with `runWithRetry`, so the dependency points
 * one way and neither side can retry the other's retry. The CLI keeps its own
 * loop (`cli/src/lib/http-retry.ts`); `samples/fixtures/retry-policy/` is the
 * table both runtimes are proven against.
 */

/** What a request cancelled before its first attempt surfaces as when the caller named no cancel class and the signal's reason is not an `Error`. */
export const RETRY_CODE_CANCELLED = "CANCELLED";

/**
 * The HTTP methods the policy and the transport both name: the replay gate
 * below reads them, and `client.ts` builds its default retry set from them.
 * One declaration so the two sets can never disagree on a spelling.
 */
export const HTTP_METHOD = {
  GET: "GET",
  HEAD: "HEAD",
  PUT: "PUT",
  DELETE: "DELETE",
} as const;

/**
 * HTTP methods safe to replay after a failure the server may have processed.
 * Mirrors the Supabase CLI's `isRetryableResponse` idempotency gate.
 */
export function isIdempotentMethod(method: string): boolean {
  const m = method.toUpperCase();
  return (
    m === HTTP_METHOD.GET || m === HTTP_METHOD.PUT || m === HTTP_METHOD.DELETE || m === HTTP_METHOD.HEAD
  );
}

const RETRY_REASON: Readonly<Record<FailureKind, RetryReason>> = {
  [FAILURE_KIND.TIMEOUT]: "timeout",
  [FAILURE_KIND.EARLY]: "425",
  [FAILURE_KIND.RATE]: "429",
  [FAILURE_KIND.SERVER]: "5xx",
  [FAILURE_KIND.NETWORK]: "network",
  [FAILURE_KIND.FATAL]: "fatal",
};

/** One attempt that threw, with what the policy needs to report it. */
class FailedAttempt {
  constructor(
    readonly failure: ClassifiedFailure,
    readonly attempt: number,
    readonly durationMs: number,
  ) {}
}

type Succeeded<T> = { value: T; durationMs: number };

/** The run's failure before any attempt has failed: none, with nothing in hand. */
const NO_FAILURE = new FailedAttempt(classifyFailure(undefined, false), 0, 0);

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

// The error a cancel surfaces as: the caller's own class when it named one,
// else the signal's reason, else the failure already in hand — and, for a
// request cancelled before it ever ran, the policy's own cancel code.
function cancelledError<T>(cfg: ResolvedRetry<T>, inHand: unknown): unknown {
  if (cfg.cancelled) return cfg.cancelled();
  const reason: unknown = cfg.signal?.reason;
  if (reason instanceof Error) return reason;
  return inHand === undefined ? new ApiError("request cancelled before its first attempt", 0, RETRY_CODE_CANCELLED) : inHand;
}

// A clock whose sleep is the run's. Everything else — the time the schedule
// measures elapsed by — stays the runtime's, inherited by prototype rather
// than copied, so no member is re-spelled here.
function clockSleepingWith(base: Clock.Clock, sleep: (duration: Duration.Duration) => Effect.Effect<void>): Clock.Clock {
  const clock: Clock.Clock = Object.create(base);
  clock.sleep = sleep;
  return clock;
}

/** One request's run under the policy: the attempts it made and what they threw. */
class RetryRun<T> {
  readonly #cfg: ResolvedRetry<T>;
  readonly #method: string;
  readonly #attempt: (remainingMs: number) => Promise<T>;
  readonly #startedAt = Date.now();
  #attemptNumber = 0;
  #last: FailedAttempt = NO_FAILURE;
  #answered: Succeeded<T> | undefined;

  constructor(cfg: ResolvedRetry<T>, method: string, attempt: (remainingMs: number) => Promise<T>) {
    this.#cfg = cfg;
    this.#method = method;
    this.#attempt = attempt;
  }

  // What is left of the deadline when this attempt starts. The attempt is
  // told, so its own ceiling can be no longer than that: the deadline then
  // bounds the last attempt too, not only the decision to begin it.
  #remainingMs(now: number): number {
    return Math.max(0, this.#cfg.deadlineMs - (now - this.#startedAt));
  }

  #attemptOnce(): Effect.Effect<Succeeded<T>, FailedAttempt> {
    return Effect.suspend(() => {
      this.#attemptNumber += 1;
      const attemptStartedAt = Date.now();
      const remainingMs = this.#remainingMs(attemptStartedAt);
      return Effect.tryPromise({
        try: () => this.#attempt(remainingMs),
        catch: (cause) => {
          this.#last = new FailedAttempt(classifyFailure(cause, false), this.#attemptNumber, Date.now() - attemptStartedAt);
          return this.#last;
        },
      }).pipe(
        Effect.map((value) => {
          this.#answered = { value, durationMs: Date.now() - attemptStartedAt };
          return this.#answered;
        }),
      );
    });
  }

  #withinRetryAfterCap(failure: ClassifiedFailure): boolean {
    return failure.retryAfterMs === null || failure.retryAfterMs <= this.#cfg.retryAfterCapMs;
  }

  #mayRetry({ failure }: FailedAttempt): boolean {
    return (
      !this.#cfg.signal?.aborted &&
      failure.kind !== FAILURE_KIND.FATAL &&
      this.#withinRetryAfterCap(failure) &&
      (failure.provenance !== PROVENANCE.POST_SEND || isIdempotentMethod(this.#method))
    );
  }

  // The deadline bounds the sleep, not only the decision: a wait that would
  // end past it is never taken, so no retry begins after `deadlineMs`.
  #withinDeadline(delay: Duration.Duration): boolean {
    return Date.now() - this.#startedAt + Duration.toMillis(delay) < this.#cfg.deadlineMs;
  }

  // A `Retry-After` the server asked for is the sleep, exactly. Otherwise the
  // exponential delay is capped and drawn with full jitter in [0, delay], so a
  // herd of renders that failed together does not retry together.
  #delayFor({ failure }: FailedAttempt, computed: Duration.Duration): Effect.Effect<Duration.Duration> {
    return Effect.sync(() => {
      if (failure.retryAfterMs !== null && failure.retryAfterMs > 0) return Duration.millis(failure.retryAfterMs);
      const capped = Math.min(Duration.toMillis(computed), this.#cfg.capDelayMs);
      return Duration.millis(this.#cfg.randomFn() * capped);
    });
  }

  #emitRetry(failed: FailedAttempt, delay: Duration.Duration): void {
    if (this.#cfg.onRetry) {
      this.#cfg.onRetry({
        attempt: failed.attempt,
        status: failed.failure.status,
        durationMs: failed.durationMs,
        reason: RETRY_REASON[failed.failure.kind],
        delayMs: Duration.toMillis(delay),
      });
    }
  }

  // The gate runs before the ceiling, the ceiling before the delay, the
  // deadline over the delay chosen, and telemetry last: a step that any of
  // them ends never reports a retry.
  #schedule() {
    return Schedule.exponential(Duration.millis(this.#cfg.baseDelayMs)).pipe(
      Schedule.setInputType<FailedAttempt>(),
      Schedule.while(({ input }) => this.#mayRetry(input)),
      Schedule.upTo({ times: this.#cfg.maxAttempts - 1 }),
      Schedule.modifyDelay(({ input, duration }) => this.#delayFor(input, duration)),
      Schedule.while(({ duration }) => this.#withinDeadline(duration)),
      Schedule.tap(({ input, duration }) => Effect.sync(() => this.#emitRetry(input, duration))),
    );
  }

  // The runtime's own sleep, or the caller's seam. A seam sleep in progress
  // is abandoned, not awaited, when the run is interrupted.
  #sleep(base: Clock.Clock, duration: Duration.Duration): Effect.Effect<void> {
    const seam = this.#cfg.sleep;
    return seam
      ? Effect.callback<void>((resume) => {
          seam(Duration.toMillis(duration), this.#cfg.signal).then(
            () => resume(Effect.void),
            (cause: unknown) => resume(Effect.die(cause)),
          );
        })
      : base.sleep(duration);
  }

  // Only a sleep may be interrupted. An abort that lands mid-attempt lets the
  // attempt settle and surfaces what it threw — the transport is bound to the
  // same signal and reports the cancel itself — and the gate then refuses the
  // next attempt. An abort that lands in a sleep ends the run there.
  #program(): Effect.Effect<Succeeded<T>, FailedAttempt> {
    const retried = Effect.retry(this.#attemptOnce(), this.#schedule());
    return Effect.uninterruptibleMask((restore) =>
      Effect.clockWith((base) =>
        Effect.provideService(
          retried,
          Clock.Clock,
          clockSleepingWith(base, (duration) => restore(this.#sleep(base, duration))),
        ),
      ),
    );
  }

  async run(): Promise<T> {
    const cfg = this.#cfg;
    // A caller that has already left gets no attempt at all.
    if (cfg.signal?.aborted) throw cancelledError(cfg, undefined);
    const exit = await Effect.runPromiseExit(this.#program(), { signal: cfg.signal });
    if (Exit.isSuccess(exit)) {
      emitTerminalAttempt(cfg.onAttempt, this.#attemptNumber, cfg.statusOf?.(exit.value.value), exit.value.durationMs);
      return exit.value.value;
    }
    if (Cause.hasInterruptsOnly(exit.cause)) {
      // A cancel that landed while the attempt was answering came too late to
      // matter: the answer is the caller's.
      const answered = this.#answered;
      if (answered) {
        emitTerminalAttempt(cfg.onAttempt, this.#attemptNumber, cfg.statusOf?.(answered.value), answered.durationMs);
        return answered.value;
      }
      // Otherwise the cancel landed in a backoff, which only follows a failure.
      const last = this.#last;
      emitTerminalAttempt(cfg.onAttempt, last.attempt, last.failure.status, last.durationMs);
      throw cancelledError(cfg, last.failure.cause);
    }
    const error = Cause.squash(exit.cause);
    if (error instanceof FailedAttempt) {
      emitTerminalAttempt(cfg.onAttempt, error.attempt, error.failure.status, error.durationMs);
      throw error.failure.cause;
    }
    // A defect: a telemetry hook that threw. It surfaces as its own error.
    throw error;
  }
}

/**
 * Runs one attempt under the policy. `method` decides the replay gate: a
 * non-idempotent method is sent again only when the failure provably happened
 * before the request left, or when the server answered without processing it.
 * Each attempt is told how much of `deadlineMs` remains when it starts, so a
 * transport can cap its own per-attempt timeout to that and the deadline
 * bounds the whole run, last attempt included; an attempt is free to ignore
 * it. On success the attempt's value is returned as is. When the schedule
 * ends — a fatal failure, the attempt ceiling, the deadline, a `Retry-After`
 * beyond the cap — the last error is re-thrown as the attempt threw it.
 */
export async function runWithRetry<T>(
  attempt: (remainingMs: number) => Promise<T>,
  method: string,
  options: RetryOptions<T> = {},
): Promise<T> {
  return new RetryRun(resolveRetryConfig(options), method, attempt).run();
}
