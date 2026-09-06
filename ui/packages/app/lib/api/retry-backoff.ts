/**
 * The waiting half of the retry policy: how long the next attempt waits, and
 * how that wait ends early when the caller cancels. Split from `retry.ts` so
 * the decision loop stays readable on its own; nothing here knows about
 * errors, methods, or attempts — only delays and signals.
 */

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

// The timer is cleared on abort, not left to run out: a backoff that follows
// a long Retry-After would otherwise hold its closure — and a live handle —
// for the whole delay after the caller has already gone.
export function defaultSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const onAbort = () => {
      clearTimeout(timer);
      resolve();
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** Sleeps for `ms`, or until `signal` aborts — whichever comes first. */
export function sleepUnlessAborted(
  sleep: (ms: number, signal?: AbortSignal) => Promise<void>,
  ms: number,
  signal: AbortSignal | undefined,
): Promise<void> {
  if (signal === undefined) return sleep(ms);
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const onAbort = () => resolve();
    signal.addEventListener("abort", onAbort, { once: true });
    void sleep(ms, signal).then(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    });
  });
}
