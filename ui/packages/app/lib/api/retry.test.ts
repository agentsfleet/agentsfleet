import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import {
  RETRY_DEFAULTS,
  backoffDelay,
  classifyRetryable,
  requestWithRetry,
} from "./retry";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => fetchMock.mockReset());
afterEach(() => fetchMock.mockReset());

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
  it("classifies TIMEOUT as 'timeout'", () => {
    const err = new ApiError("timed out", 408, "TIMEOUT");
    expect(classifyRetryable(err)).toBe("timeout");
  });
  it("classifies server retry-marker code (UZ-*-RETRY*) as 'server_marked_retryable'", () => {
    const err = new ApiError("hint", 400, "UZ-EXEC-RETRY-A");
    expect(classifyRetryable(err)).toBe("server_marked_retryable");
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
      retryAfterMs: 1000,
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
    expect(onAttempt.mock.calls[0]![0]).toMatchObject({
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
    expect(onRetry.mock.calls[0]![0]).toMatchObject({
      attempt: 1,
      status: 503,
      reason: "5xx",
    });
    expect(onAttempt).toHaveBeenCalledTimes(1);
    expect(onAttempt.mock.calls[0]![0]).toMatchObject({
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

  it("retries up to maxAttempts then throws the last error", async () => {
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
    expect(onAttempt.mock.calls[0]![0]).toMatchObject({
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
    expect(onRetry.mock.calls[0]![0]).toMatchObject({ reason: "network" });
  });
});

describe("requestWithRetry — config", () => {
  it("rejects maxAttempts < 1", async () => {
    await expect(
      requestWithRetry("/v1/x", { method: "GET" }, "tok", {
        maxAttempts: 0,
        sleepImpl: NOOP_SLEEP,
      }),
    ).rejects.toMatchObject({ code: "CONFIG_INVALID" });
  });
  it("rejects maxAttempts > hard cap", async () => {
    await expect(
      requestWithRetry("/v1/x", { method: "GET" }, "tok", {
        maxAttempts: RETRY_DEFAULTS.hardCap + 1,
        sleepImpl: NOOP_SLEEP,
      }),
    ).rejects.toMatchObject({ code: "CONFIG_INVALID" });
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

  it("ZOMBIE_NO_RETRY=1 collapses maxAttempts to a single attempt", async () => {
    const prev = process.env.ZOMBIE_NO_RETRY;
    process.env.ZOMBIE_NO_RETRY = "1";
    try {
      fetchMock.mockResolvedValue(jsonResponse(503, { detail: "svc" }));
      const onRetry = vi.fn();
      await expect(
        requestWithRetry("/v1/x", { method: "GET" }, "tok", {
          maxAttempts: 3,
          onRetry,
          sleepImpl: NOOP_SLEEP,
          randomFn: NOOP_RANDOM,
        }),
      ).rejects.toBeInstanceOf(ApiError);
      expect(fetchMock).toHaveBeenCalledTimes(1); // no retry despite a 503
      expect(onRetry).not.toHaveBeenCalled();
    } finally {
      if (prev === undefined) delete process.env.ZOMBIE_NO_RETRY;
      else process.env.ZOMBIE_NO_RETRY = prev;
    }
  });
});
