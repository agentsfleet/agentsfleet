import { describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import {
  RETRY_CODE_TIMEOUT,
  RETRY_DEFAULTS,
  backoffDelay,
  classifyRetryable,
  isIdempotentMethod,
  runWithRetry,
} from "./retry";

// The transport-level proofs (a real `fetch`, a real `ApiError` off the wire)
// live beside the transport in client.retry.test.ts and client.defaults.test.ts;
// this file covers the policy in isolation.

const NOOP_SLEEP = (_ms: number) => Promise.resolve();
const NOOP_RANDOM = () => 0;
const MS_PER_SECOND = 1000 as const;
const TRANSIENT_STATUS = 503;

describe("classifyRetryable", () => {
  it("classifies ApiError 429 as '429'", () => {
    const err = new ApiError("rate limited", 429, "UZ-RATE-001");
    expect(classifyRetryable(err)).toBe("429");
  });
  it("classifies ApiError 503 / 502 / 504 / 408 / 425 as '5xx'", () => {
    for (const s of [502, 503, 504, 408, 425]) {
      const err = new ApiError("svc", s, "X");
      expect(classifyRetryable(err)).toBe("5xx");
    }
  });
  it("classifies the transport's timeout code as 'timeout'", () => {
    const err = new ApiError("timed out", 408, RETRY_CODE_TIMEOUT);
    expect(classifyRetryable(err)).toBe("timeout");
  });
  it("classifies fetch-failed TypeError as 'network'", () => {
    expect(classifyRetryable(new TypeError("fetch failed"))).toBe("network");
  });
  it("returns null for non-retryable ApiError (400/401/404)", () => {
    expect(classifyRetryable(new ApiError("bad", 400, "X"))).toBeNull();
    expect(classifyRetryable(new ApiError("auth", 401, "X"))).toBeNull();
    expect(classifyRetryable(new ApiError("nf", 404, "X"))).toBeNull();
  });
  it("returns null for non-Error values", () => {
    expect(classifyRetryable("string")).toBeNull();
    expect(classifyRetryable(null)).toBeNull();
  });
  it("classifies node-shaped socket errors (ECONNRESET/ETIMEDOUT/ENOTFOUND) as 'network'", () => {
    for (const code of ["ECONNRESET", "ETIMEDOUT", "ENOTFOUND"]) {
      expect(classifyRetryable({ code })).toBe("network");
    }
  });
  it("returns null for an unrecognized node error code", () => {
    expect(classifyRetryable({ code: "EPERM" })).toBeNull();
  });
});

describe("backoffDelay", () => {
  it("honors server Retry-After floor with +20% jitter cap", () => {
    const d = backoffDelay({
      attempt: 1,
      baseDelayMs: 250,
      capDelayMs: 2000,
      retryAfterMs: MS_PER_SECOND,
      randomFn: () => 1, // maximizes jitter add
    });
    // Retry-After 1000ms + (1000 * 0.2 * 1) = 1200
    expect(d).toBeCloseTo(1200, 0);
  });
  it("applies exponential backoff capped at capDelayMs", () => {
    const d = backoffDelay({
      attempt: 10,
      baseDelayMs: 250,
      capDelayMs: 2000,
      retryAfterMs: null,
      randomFn: () => 0.5, // jitter = 0 (centered)
    });
    // base = min(250 * 2^9, 2000) = 2000; jitter = 2000 * 0.2 * 0 = 0
    expect(d).toBe(2000);
  });
});

describe("isIdempotentMethod", () => {
  it("GET/PUT/DELETE/HEAD safe; POST/PATCH not (case-insensitive)", () => {
    for (const m of ["GET", "put", "Delete", "HEAD"]) {
      expect(isIdempotentMethod(m)).toBe(true);
    }
    for (const m of ["POST", "patch", "CONNECT"]) {
      expect(isIdempotentMethod(m)).toBe(false);
    }
  });
});

describe("runWithRetry — the loop over an attempt thunk", () => {
  it("runs the attempt exactly maxAttempts times on a persistent transient failure", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
    const attempt = vi.fn().mockRejectedValue(new ApiError("svc", TRANSIENT_STATUS, "X"));
    await expect(
      runWithRetry(attempt, "GET", { maxAttempts: 3, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM }),
    ).rejects.toBeInstanceOf(ApiError);
    expect(attempt).toHaveBeenCalledTimes(3);
    vi.unstubAllEnvs();
  });

  it("a backoff in progress ends when the caller cancels, and no attempt follows", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
    vi.useFakeTimers();
    const controller = new AbortController();
    const attempt = vi.fn().mockRejectedValue(new ApiError("svc", TRANSIENT_STATUS, "X"));
    // Real (faked) sleep: a long base so the abort lands inside the backoff.
    const settled = runWithRetry(attempt, "GET", {
      baseDelayMs: 60_000,
      capDelayMs: 60_000,
      randomFn: NOOP_RANDOM,
      signal: controller.signal,
    }).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempt).toHaveBeenCalledTimes(1);
    controller.abort();
    await vi.advanceTimersByTimeAsync(1);
    expect(await settled).toBeInstanceOf(ApiError);
    expect(attempt).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
    vi.unstubAllEnvs();
  });

  it("returns the attempt's value untouched on success", async () => {
    const attempt = vi.fn().mockResolvedValue({ etag: "v1", data: 42 });
    await expect(runWithRetry(attempt, "GET", { sleepImpl: NOOP_SLEEP })).resolves.toEqual({
      etag: "v1",
      data: 42,
    });
    expect(attempt).toHaveBeenCalledTimes(1);
  });

  it("rejects maxAttempts < 1", async () => {
    await expect(
      runWithRetry(async () => 1, "GET", { maxAttempts: 0, sleepImpl: NOOP_SLEEP }),
    ).rejects.toMatchObject({ code: "CONFIG_INVALID" });
  });

  it("rejects maxAttempts > hard cap", async () => {
    await expect(
      runWithRetry(async () => 1, "GET", {
        maxAttempts: RETRY_DEFAULTS.hardCap + 1,
        sleepImpl: NOOP_SLEEP,
      }),
    ).rejects.toMatchObject({ code: "CONFIG_INVALID" });
  });

  it("AGENTSFLEET_NO_RETRY=1 collapses maxAttempts to a single attempt", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "1");
    const attempt = vi.fn().mockRejectedValue(new ApiError("svc", TRANSIENT_STATUS, "X"));
    const onRetry = vi.fn();
    await expect(
      runWithRetry(attempt, "GET", { maxAttempts: 3, onRetry, sleepImpl: NOOP_SLEEP }),
    ).rejects.toBeInstanceOf(ApiError);
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
    vi.unstubAllEnvs();
  });
});
