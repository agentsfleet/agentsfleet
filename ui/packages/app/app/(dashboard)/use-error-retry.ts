"use client";

import { useCallback, useEffect, useRef, useState } from "react";

// Automatic recovery for the dashboard error boundary.
//
// The boundary's own copy calls the failure transient, and a transient failure
// that waits for a human to click Retry is one the product could have cleared
// itself. This owns the waiting: it counts down, calls `reset()`, and — when a
// retry fails, remounting the boundary — resumes at the NEXT delay rather than
// restarting the ladder, so a persistently broken route backs off instead of
// hammering the server every few seconds.
//
// Attempt state survives the remount because the boundary is re-created on each
// failure: a plain `useState` would reset to zero every time and the backoff
// would never actually grow. The counter therefore lives at module scope —
// there is one dashboard boundary mounted at a time.
//
// Which raises the question the counter cannot answer on its own: a boundary
// that unmounts because the retry WORKED looks exactly like one that unmounts
// because it is about to remount on a new failure. Left alone, the counter
// would stay elevated after every successful recovery, so an unrelated failure
// ten minutes later would start mid-ladder and, eventually, never auto-retry at
// all. `INCIDENT_WINDOW_MS` settles it by elapsed time instead: failures that
// arrive close together are the same incident and keep backing off; one that
// arrives long after the last is a new incident and starts fresh.

/** Delay before each automatic attempt. Its length IS the attempt budget. */
export const RETRY_DELAYS_MS: readonly number[] = [3_000, 6_000, 12_000];

/** How often the countdown label re-renders. One second, so it reads as seconds. */
const TICK_MS = 1_000;

const MS_PER_SECOND = 1_000;

/**
 * Silence that ends an incident. Comfortably longer than the last rung, so a
 * retry that fails after its full wait is still counted as the same incident.
 */
export const INCIDENT_WINDOW_MS = 30_000;

// Survive the boundary remount that a failed retry causes.
let attemptsUsed = 0;
let lastFailureAt = 0;

/** Test seam: this state is intentionally module-level, not React state. */
export function __resetErrorRetryForTests(): void {
  attemptsUsed = 0;
  lastFailureAt = 0;
}

export type ErrorRetry = {
  /** 1-based attempt about to be made, or `null` once the budget is spent. */
  attempt: number | null;
  /** Total automatic attempts this boundary will make. */
  maxAttempts: number;
  /** Whole seconds until the next automatic attempt; 0 when none is pending. */
  secondsRemaining: number;
  /** Fraction of the current wait already elapsed, for the progress ring. */
  progress: number;
  /** Retry now, cancelling any pending countdown. */
  retryNow: () => void;
  /** True once the automatic attempts are spent and only manual retry remains. */
  exhausted: boolean;
};

export function useErrorRetry(reset: () => void): ErrorRetry {
  // Read once per mount: this mount represents one failure, and the value must
  // not change under the countdown that is already scheduled against it. The
  // staleness check runs in the same initializer so it is decided before any
  // timer is scheduled against the position it returns.
  const [attemptIndex] = useState(() => {
    const now = Date.now();
    if (lastFailureAt !== 0 && now - lastFailureAt > INCIDENT_WINDOW_MS) attemptsUsed = 0;
    lastFailureAt = now;
    return attemptsUsed;
  });
  const delay = RETRY_DELAYS_MS[attemptIndex];
  const exhausted = delay === undefined;

  const [remainingMs, setRemainingMs] = useState(delay ?? 0);
  const resetRef = useRef(reset);
  resetRef.current = reset;

  const retryNow = useCallback(() => {
    // A manual retry restarts the ladder: the user has taken over, and making
    // their next automatic wait 12s because the machine already tried twice
    // would punish them for helping.
    attemptsUsed = 0;
    lastFailureAt = 0;
    resetRef.current();
  }, []);

  useEffect(() => {
    if (delay === undefined) return;

    const startedAt = performance.now();
    const tick = setInterval(() => {
      setRemainingMs(Math.max(0, delay - (performance.now() - startedAt)));
    }, TICK_MS);
    const fire = setTimeout(() => {
      // Spend the attempt BEFORE resetting: if the reset throws straight back
      // into this boundary, the remount must see the incremented count or the
      // ladder never advances.
      attemptsUsed = attemptIndex + 1;
      resetRef.current();
    }, delay);

    return () => {
      clearInterval(tick);
      clearTimeout(fire);
    };
  }, [attemptIndex, delay]);

  return {
    attempt: exhausted ? null : attemptIndex + 1,
    maxAttempts: RETRY_DELAYS_MS.length,
    secondsRemaining: Math.ceil(remainingMs / MS_PER_SECOND),
    progress: delay === undefined ? 1 : 1 - remainingMs / delay,
    retryNow,
    exhausted,
  };
}
