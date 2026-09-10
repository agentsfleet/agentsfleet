import { describe, expect, it } from "vitest";
import { RETRY_DEFAULTS } from "../lib/api/retry";
import { DEFAULT_REQUEST_TIMEOUT_MS, attemptTimeoutMs } from "../lib/api/client";

/**
 * The worst case a dynamic server render can wait on one retried read, computed
 * from the policy's exported defaults rather than typed into prose.
 *
 * Two clamps in two modules make the deadline a true total, and neither is
 * obvious from the constant alone:
 *  - `retry.ts` `#withinDeadline` refuses a sleep that would END past the
 *    deadline, so no attempt BEGINS after it.
 *  - `client.ts` `attemptTimeoutMs` gives an attempt only what REMAINS of the
 *    deadline, so the attempt that did begin cannot outlive it either.
 *
 * Drop either clamp and the ceiling becomes `deadlineMs + DEFAULT_REQUEST_TIMEOUT_MS`.
 * `UNCLAMPED_CEILING_MS` below is that discarded number, kept as the thing the
 * bound must stay strictly under — which is what makes this a pin rather than a
 * restatement of `deadlineMs`.
 */

// Attempt start offsets the supremum is probed at: the first attempt, the point
// where the remaining budget drops below one full request ceiling, and a start
// arbitrarily close to the deadline. The third is the case the clamp exists for.
const ONE_MS = 1;
const PROBE_START_OFFSETS_MS = [
  0,
  DEFAULT_REQUEST_TIMEOUT_MS,
  RETRY_DEFAULTS.deadlineMs - ONE_MS,
] as const;

const UNCLAMPED_CEILING_MS = RETRY_DEFAULTS.deadlineMs + DEFAULT_REQUEST_TIMEOUT_MS;

/** When an attempt begun at `startedAtMs` can latest return, under both clamps. */
function latestCompletionMs(startedAtMs: number): number {
  const remainingMs = Math.max(0, RETRY_DEFAULTS.deadlineMs - startedAtMs);
  return startedAtMs + attemptTimeoutMs(remainingMs);
}

/** The ladder's ceiling: the largest completion any legal attempt start yields. */
function worstCaseWaitMs(): number {
  return Math.max(...PROBE_START_OFFSETS_MS.map(latestCompletionMs));
}

describe("retry ladder bound", () => {
  it("the render wait is bounded by the declared retry ladder", () => {
    expect(worstCaseWaitMs()).toBe(RETRY_DEFAULTS.deadlineMs);
  });

  it("the per-attempt ceiling never pushes a run past its deadline", () => {
    for (const startedAtMs of PROBE_START_OFFSETS_MS) {
      expect(
        latestCompletionMs(startedAtMs),
        `an attempt begun at ${startedAtMs}ms must not outlive the deadline`,
      ).toBeLessThanOrEqual(RETRY_DEFAULTS.deadlineMs);
    }
  });

  it("the bound is the deadline because of the clamp, not the deadline alone", () => {
    // Without `attemptTimeoutMs`'s remaining-budget clamp a late attempt would
    // run its full ceiling past the deadline. Asserting the gap is what fails
    // if that clamp is ever removed.
    expect(worstCaseWaitMs()).toBeLessThan(UNCLAMPED_CEILING_MS);
    expect(attemptTimeoutMs(ONE_MS)).toBe(ONE_MS);
    expect(attemptTimeoutMs(UNCLAMPED_CEILING_MS)).toBe(DEFAULT_REQUEST_TIMEOUT_MS);
  });

  it("the attempt count is bounded by the declared ceiling", () => {
    expect(RETRY_DEFAULTS.maxAttempts).toBeLessThanOrEqual(RETRY_DEFAULTS.hardCap);
    expect(RETRY_DEFAULTS.maxAttempts).toBeGreaterThan(0);
  });
});
