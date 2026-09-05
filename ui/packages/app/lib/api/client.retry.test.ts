import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { requestWithRetry } from "./client";
import { RETRY_CODE_TIMEOUT } from "./retry";

// `requestWithRetry` is the explicit opt-in: the caller owns its replay
// decision and passes the policy options. These cases drive it through the
// real transport (`fetch` stubbed, everything else live) so an `ApiError` off
// the wire — status, Retry-After — is what the policy classifies.

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

// The suite default is one attempt (vitest.setup.ts). This file proves the
// policy, so it turns retry back on per test and restores the default after.
beforeEach(() => {
  fetchMock.mockReset();
  vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
});
afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllEnvs();
});

function jsonResponse(status: number, body: unknown, retryAfter?: string) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: {
      get: (k: string) => (k.toLowerCase() === "retry-after" ? retryAfter ?? null : null),
    },
    json: async () => body,
  };
}

// Sleep stub so the test runs in microseconds, not real wall time.
const NOOP_SLEEP = (_ms: number) => Promise.resolve();
const NOOP_RANDOM = () => 0; // deterministic jitter

describe("requestWithRetry — happy path", () => {
  it("returns body on first 200, fires onAttempt(terminal=true) once", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { event_id: "evt_1" }));
    const onAttempt = vi.fn();
    const onRetry = vi.fn();
    const result = await requestWithRetry<{ event_id: string }>(
      "/v1/whatever",
      { method: "POST" },
      "tok",
      { onAttempt, onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
    );
    expect(result.event_id).toBe("evt_1");
    expect(onRetry).not.toHaveBeenCalled();
    expect(onAttempt).toHaveBeenCalledTimes(1);
    expect(onAttempt.mock.calls[0]?.[0]).toMatchObject({
      attempt: 1,
      terminal: true,
      retryCount: 0,
    });
  });
});

describe("requestWithRetry — retries", () => {
  it("retries on 503 then succeeds; fires onRetry once + onAttempt(terminal) once", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(503, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    const onAttempt = vi.fn();
    const onRetry = vi.fn();
    const result = await requestWithRetry<{ ok: number }>(
      "/v1/x",
      { method: "GET" },
      "tok",
      { onAttempt, onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
    );
    expect(result.ok).toBe(1);
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry.mock.calls[0]?.[0]).toMatchObject({
      attempt: 1,
      status: 503,
      reason: "5xx",
    });
    expect(onAttempt).toHaveBeenCalledTimes(1);
    expect(onAttempt.mock.calls[0]?.[0]).toMatchObject({
      attempt: 2,
      terminal: true,
    });
  });

  it("honors Retry-After header on 429", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(429, { detail: "slow" }, "5"));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    let slept = 0;
    const sleep = (ms: number) => {
      slept = ms;
      return Promise.resolve();
    };
    await requestWithRetry<{ ok: number }>(
      "/v1/x",
      { method: "GET" },
      "tok",
      { sleepImpl: sleep, randomFn: NOOP_RANDOM },
    );
    // 5s = 5000ms floor + 0 jitter (randomFn=0)
    expect(slept).toBe(5000);
  });

  it("does NOT retry on 400 (non-retryable)", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse(400, { detail: "bad", error_code: "UZ-VALIDATE-001" }),
    );
    const onRetry = vi.fn();
    await expect(
      requestWithRetry(
        "/v1/x",
        { method: "POST" },
        "tok",
        { onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
      ),
    ).rejects.toBeInstanceOf(ApiError);
    expect(onRetry).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("an explicit retry caller never retries twice", async () => {
    // The transport wraps the single attempt exactly once: maxAttempts 3 is
    // three fetches, never nine.
    fetchMock.mockResolvedValue(jsonResponse(503, { detail: "svc" }));
    const onAttempt = vi.fn();
    const onRetry = vi.fn();
    await expect(
      requestWithRetry(
        "/v1/x",
        { method: "GET" },
        "tok",
        {
          maxAttempts: 3,
          onAttempt,
          onRetry,
          sleepImpl: NOOP_SLEEP,
          randomFn: NOOP_RANDOM,
        },
      ),
    ).rejects.toBeInstanceOf(ApiError);
    // 3 fetches, 2 retries fired between them, 1 terminal onAttempt at end.
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(onRetry).toHaveBeenCalledTimes(2);
    expect(onAttempt).toHaveBeenCalledTimes(1);
    expect(onAttempt.mock.calls[0]?.[0]).toMatchObject({
      attempt: 3,
      terminal: true,
    });
  });

  it("retries on fetch network failure (TypeError 'fetch failed')", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("fetch failed"));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    const onRetry = vi.fn();
    const result = await requestWithRetry<{ ok: number }>(
      "/v1/x",
      { method: "GET" },
      "tok",
      { onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
    );
    expect(result.ok).toBe(1);
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onRetry.mock.calls[0]?.[0]).toMatchObject({ reason: "network" });
  });

  it("uses the built-in sleep when no sleepImpl is injected", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(503, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    // Tiny base/cap so the real setTimeout-backed sleep returns in ~1ms.
    const result = await requestWithRetry<{ ok: number }>(
      "/v1/x",
      { method: "GET" },
      "tok",
      { baseDelayMs: 1, capDelayMs: 1, randomFn: NOOP_RANDOM },
    );
    expect(result.ok).toBe(1);
  });
});

describe("requestWithRetry — idempotency guard", () => {
  it("does NOT retry a POST that returns 503 (duplicate-mutation hazard)", async () => {
    fetchMock.mockResolvedValue(jsonResponse(503, { detail: "svc" }));
    const onRetry = vi.fn();
    await expect(
      requestWithRetry(
        "/v1/x",
        { method: "POST" },
        "tok",
        { maxAttempts: 3, onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
      ),
    ).rejects.toBeInstanceOf(ApiError);
    // One attempt only — the 503 is non-idempotent, so no replay.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
  });

  it("DOES retry a PUT that returns 503 (idempotent method)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(503, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    const onRetry = vi.fn();
    const result = await requestWithRetry<{ ok: number }>(
      "/v1/x",
      { method: "PUT" },
      "tok",
      { onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
    );
    expect(result.ok).toBe(1);
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("does NOT replay a POST whose attempt timed out client-side (it may have been processed)", async () => {
    // The default per-attempt timeout fires with the request on the wire. A
    // server 408 (below) is the server declining the request; this is the
    // client giving up on an answer that may still be coming.
    fetchMock.mockRejectedValue(new DOMException("signal timed out", "TimeoutError"));
    const onRetry = vi.fn();
    await expect(
      requestWithRetry("/v1/x", { method: "POST" }, "tok", {
        onRetry,
        sleepImpl: NOOP_SLEEP,
        randomFn: NOOP_RANDOM,
      }),
    ).rejects.toMatchObject({ code: RETRY_CODE_TIMEOUT });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
  });

  it("DOES retry a POST that returns 429 or 408 (request not processed)", async () => {
    for (const status of [429, 408]) {
      fetchMock.mockReset();
      fetchMock.mockResolvedValueOnce(jsonResponse(status, { detail: "x" }));
      fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
      const onRetry = vi.fn();
      await requestWithRetry(
        "/v1/x",
        { method: "POST" },
        "tok",
        { onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
      );
      expect(onRetry).toHaveBeenCalledTimes(1);
    }
  });

  it("treats an omitted method as GET (idempotent) so reads still retry on 5xx", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(503, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { ok: 1 }));
    const onRetry = vi.fn();
    // No `method` in init → defaults to GET → 5xx is replay-safe.
    const result = await requestWithRetry<{ ok: number }>(
      "/v1/x",
      {},
      "tok",
      { onRetry, sleepImpl: NOOP_SLEEP, randomFn: NOOP_RANDOM },
    );
    expect(result.ok).toBe(1);
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});
