import { describe, expect, it, vi } from "vitest";
import { ApiError, HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT, RequestCancelledError } from "./errors";
import { RETRY_DEFAULTS, isIdempotentMethod, runWithRetry } from "./retry";

// The transport-level proofs (a real `fetch`, a real `ApiError` off the wire)
// live beside the transport in client.retry.test.ts and client.defaults.test.ts;
// this file covers the policy's loop over a bare thunk. The schedule's own
// guarantees — deadline, Retry-After cap, full jitter, provenance, telemetry —
// are proved in retry.schedule.test.ts, and the classifier in
// retry-classify.test.ts.

const NOOP_SLEEP = (_ms: number) => Promise.resolve();
const NOOP_RANDOM = () => 0;
// Full jitter draws the delay from [0, delay]; a draw of 1 is the bare delay,
// which is what a test that must land an abort inside a backoff needs.
const BARE_DELAY = () => 1;
// A backoff long enough to land an abort inside, with a deadline that allows it.
const LONG_BACKOFF_MS = 60_000;
const TRANSIENT_STATUS = 503;

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

describe("runWithRetry — the replay gate", () => {
  it("a client-side timeout never replays a non-idempotent method, and still retries a read", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
    const timedOut = () => new ApiError("timed out", HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT);
    const policy = { maxAttempts: 3, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM };
    // The request was on the wire when the clock ran out: the server may have
    // processed it, so a write gets no second copy.
    for (const method of ["POST", "PATCH"]) {
      const write = vi.fn().mockRejectedValue(timedOut());
      await expect(runWithRetry(write, method, policy)).rejects.toMatchObject({
        code: RETRY_CODE_TIMEOUT,
      });
      expect(write, method).toHaveBeenCalledTimes(1);
    }
    // A read is replay-safe and rides the full attempt ceiling.
    const read = vi.fn().mockRejectedValue(timedOut());
    await expect(runWithRetry(read, "GET", policy)).rejects.toMatchObject({ code: RETRY_CODE_TIMEOUT });
    expect(read).toHaveBeenCalledTimes(3);
    vi.unstubAllEnvs();
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
      baseDelayMs: LONG_BACKOFF_MS,
      capDelayMs: LONG_BACKOFF_MS,
      deadlineMs: 2 * LONG_BACKOFF_MS,
      randomFn: BARE_DELAY,
      signal: controller.signal,
    }).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempt).toHaveBeenCalledTimes(1);
    controller.abort();
    await vi.advanceTimersByTimeAsync(1);
    // What surfaces is the cancel, not the 503 that preceded it, and the
    // backoff timer is gone rather than left to run out.
    expect(await settled).toBe(controller.signal.reason);
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
    vi.unstubAllEnvs();
  });

  it("a cancel whose reason is not an Error surfaces the failure already in hand", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
    vi.useFakeTimers();
    const controller = new AbortController();
    const refused = new ApiError("svc", TRANSIENT_STATUS, "X");
    const attempt = vi.fn().mockRejectedValue(refused);
    const settled = runWithRetry(attempt, "GET", {
      baseDelayMs: LONG_BACKOFF_MS,
      capDelayMs: LONG_BACKOFF_MS,
      deadlineMs: 2 * LONG_BACKOFF_MS,
      randomFn: BARE_DELAY,
      signal: controller.signal,
    }).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(1);
    controller.abort("the page moved on");
    await vi.advanceTimersByTimeAsync(1);
    expect(await settled).toBe(refused);
    vi.useRealTimers();
    vi.unstubAllEnvs();
  });

  it("runs with the policy's own defaults when the caller names none", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "1");
    const attempt = vi.fn().mockResolvedValue("ok");
    expect(await runWithRetry(attempt, "GET")).toBe("ok");
    expect(attempt).toHaveBeenCalledTimes(1);
    vi.unstubAllEnvs();
  });

  it("a cancel during backoff throws the caller's own cancel class when one is named", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
    vi.useFakeTimers();
    const controller = new AbortController();
    const attempt = vi.fn().mockRejectedValue(new ApiError("svc", TRANSIENT_STATUS, "X"));
    const settled = runWithRetry(attempt, "GET", {
      baseDelayMs: LONG_BACKOFF_MS,
      capDelayMs: LONG_BACKOFF_MS,
      deadlineMs: 2 * LONG_BACKOFF_MS,
      randomFn: BARE_DELAY,
      signal: controller.signal,
      cancelled: () => new RequestCancelledError("/v1/x"),
    }).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(1);
    controller.abort();
    await vi.advanceTimersByTimeAsync(1);
    expect(await settled).toBeInstanceOf(RequestCancelledError);
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
