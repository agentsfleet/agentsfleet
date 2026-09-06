import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, RETRY_CODE_TIMEOUT, RequestCancelledError } from "./errors";
import { attemptTimeoutMs, DEFAULT_REQUEST_TIMEOUT_MS, request, requestWithRetry } from "./client";
import { RETRY_DEFAULTS } from "./retry";
import {
  readWorkspaceFetchAudit,
  resetWorkspaceFetchAudit,
  WORKSPACE_LIST_PATH,
} from "../acceptance/workspace-fetch-audit";

// What `request()` does with NO options: every read rides the retry policy,
// every request without a caller signal carries the default timeout, and a
// write keeps one attempt. The transport's built-in sleep is real, so the
// clock is faked and advanced past the policy's cap between attempts.

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

const PATH = "/v1/thing";
const TOKEN = "tok";
const OK_BODY = { ok: 1 };
const TRANSIENT_STATUS = 503;
// Shorter than the default per-attempt timeout, so the deadline is what binds.
const SHORT_DEADLINE_MS = 1_000;
// The policy's two default backoffs (250ms, 500ms, each ±20%) both fit inside
// one cap-sized advance per gap; two gaps sit inside three attempts.
const PAST_ALL_BACKOFFS_MS = RETRY_DEFAULTS.capDelayMs * 2;

function jsonResponse(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: () => null },
    json: async () => body,
  };
}

/** What `fetch` rejects with when an `AbortSignal.timeout` fires mid-flight. */
function timeoutRejection(): DOMException {
  return new DOMException("signal timed out", "TimeoutError");
}

function abortRejection(): Error {
  const err = new Error("The operation was aborted.");
  err.name = "AbortError";
  return err;
}

/** A response whose headers arrived but whose body read rejects with `cause`. */
function bodyRejecting(status: number, cause: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: () => null },
    json: () => Promise.reject(cause),
  };
}

/** The `signal` the nth fetch was given, asserting the call happened. */
function sentSignal(callIndex: number): AbortSignal | null | undefined {
  const call = fetchMock.mock.calls[callIndex];
  expect(call, `expected a fetch call at index ${callIndex}`).toBeDefined();
  return (call?.[1] as RequestInit | undefined)?.signal;
}

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllEnvs();
  resetWorkspaceFetchAudit();
  fetchMock.mockReset();
});

describe("request — default retry", () => {
  it("request retries a transient read and returns the recovered body", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, OK_BODY));

    const pending = request<typeof OK_BODY>(PATH, { method: "GET" }, TOKEN);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    await expect(pending).resolves.toEqual(OK_BODY);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("request does not replay a non-idempotent write on a server error", async () => {
    fetchMock.mockResolvedValue(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));

    const settled = request(PATH, { method: "POST", body: "{}" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    const err = (await settled) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(TRANSIENT_STATUS);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("request does not retry a DELETE on its own — a lost 204 must not come back as a 404", async () => {
    fetchMock.mockResolvedValue(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));

    const settled = request(PATH, { method: "DELETE" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    expect(await settled).toBeInstanceOf(ApiError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("request retries a PUT — the one write that is replay-safe in effect and in answer", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, OK_BODY));

    const pending = request<typeof OK_BODY>(PATH, { method: "PUT", body: "{}" }, TOKEN);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    await expect(pending).resolves.toEqual(OK_BODY);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("the acceptance audit counts a retried read once — a retry is not a second ask", async () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    fetchMock.mockResolvedValueOnce(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));
    fetchMock.mockResolvedValueOnce(jsonResponse(200, OK_BODY));

    const pending = request(WORKSPACE_LIST_PATH, { method: "GET" }, TOKEN);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);
    await pending;

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(readWorkspaceFetchAudit()).toEqual({ total: 1, byPath: { [WORKSPACE_LIST_PATH]: 1 } });
  });

  it("no-retry env still yields one attempt", async () => {
    vi.stubEnv("AGENTSFLEET_NO_RETRY", "1");
    fetchMock.mockResolvedValue(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));

    const settled = request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    expect(await settled).toBeInstanceOf(ApiError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("request — default timeout", () => {
  it("a hung read times out into the retryable class and stops after the attempt ceiling", async () => {
    fetchMock.mockRejectedValue(timeoutRejection());

    const settled = request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    const err = (await settled) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe(RETRY_CODE_TIMEOUT);
    expect(err.message).toContain(PATH);
    expect(fetchMock).toHaveBeenCalledTimes(RETRY_DEFAULTS.maxAttempts);
  });

  it("the default timeout is no longer than what remains of the deadline", async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, OK_BODY));
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout");

    await requestWithRetry(PATH, {}, TOKEN, { deadlineMs: SHORT_DEADLINE_MS });

    // The run started moments before the attempt, so the budget it was told
    // is the short deadline less what has already elapsed — never the default.
    expect(timeoutSpy).toHaveBeenCalledTimes(1);
    const granted = timeoutSpy.mock.calls[0]?.[0];
    expect(granted).toBeLessThanOrEqual(SHORT_DEADLINE_MS);
    expect(granted).toBeGreaterThan(0);
    expect(sentSignal(0)).toBe(timeoutSpy.mock.results[0]?.value);
  });

  it("attemptTimeoutMs is the smaller of the default and the remaining budget", () => {
    expect(attemptTimeoutMs(DEFAULT_REQUEST_TIMEOUT_MS * 2)).toBe(DEFAULT_REQUEST_TIMEOUT_MS);
    expect(attemptTimeoutMs(SHORT_DEADLINE_MS)).toBe(SHORT_DEADLINE_MS);
  });

  it("a caller signal wins over the default timeout", async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, OK_BODY));
    const controller = new AbortController();
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout");

    await request(PATH, { method: "GET", signal: controller.signal }, TOKEN);
    await request(PATH, { method: "GET" }, TOKEN);

    // The caller's own signal is passed through untouched, and no default
    // timer is minted beside it…
    expect(sentSignal(0)).toBe(controller.signal);
    // …while a request with none is given the default budget, so no fetch is
    // unbounded and no other budget sneaks in.
    expect(timeoutSpy).toHaveBeenCalledTimes(1);
    expect(timeoutSpy).toHaveBeenCalledWith(DEFAULT_REQUEST_TIMEOUT_MS);
    expect(sentSignal(1)).toBe(timeoutSpy.mock.results[0]?.value);
    // pin test: literal is the contract — the SSE backfill's proxy fetch is
    // held to the same window (lib/streaming/fleet-stream-backfill.ts).
    expect(DEFAULT_REQUEST_TIMEOUT_MS).toBe(10_000);
    timeoutSpy.mockRestore();
  });

  it("a timeout during the body read is a timeout, never a success body", async () => {
    // Headers in under the budget, body not: the same signal bounds the body
    // stream, and what it raises there must classify the same way.
    fetchMock.mockResolvedValue(bodyRejecting(200, timeoutRejection()));

    const settled = request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    const err = (await settled) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe(RETRY_CODE_TIMEOUT);
    // A read: retried like a timeout before the headers.
    expect(fetchMock).toHaveBeenCalledTimes(RETRY_DEFAULTS.maxAttempts);
  });

  it("a cancel during the body read is a cancel, never a success body", async () => {
    const controller = new AbortController();
    fetchMock.mockResolvedValue(bodyRejecting(200, abortRejection()));

    const settled = request(PATH, { method: "GET", signal: controller.signal }, TOKEN).catch(
      (e: unknown) => e,
    );
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    expect(await settled).toBeInstanceOf(RequestCancelledError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("a cancel during a backoff surfaces as a cancel, not the stale status", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));
    const controller = new AbortController();

    const settled = request(PATH, { method: "GET", signal: controller.signal }, TOKEN).catch(
      (e: unknown) => e,
    );
    // The first attempt has failed and the loop is asleep between attempts.
    await vi.advanceTimersByTimeAsync(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    controller.abort(abortRejection());
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    expect(await settled).toBeInstanceOf(RequestCancelledError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("an already-cancelled caller gets no second attempt, whatever the failure class", async () => {
    // The caller's own timeout fires mid-flight: the signal is aborted AND the
    // rejection is the retryable TimeoutError. Retrying would only fail again
    // instantly against the same dead signal, after a full backoff sleep.
    const controller = new AbortController();
    fetchMock.mockImplementation(() => {
      controller.abort(timeoutRejection());
      return Promise.reject(timeoutRejection());
    });

    const settled = request(PATH, { method: "GET", signal: controller.signal }, TOKEN).catch(
      (e: unknown) => e,
    );
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    const err = (await settled) as ApiError;
    expect(err.code).toBe(RETRY_CODE_TIMEOUT);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("a navigation abort is a cancel, not a retry", async () => {
    fetchMock.mockRejectedValue(abortRejection());

    const settled = request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(PAST_ALL_BACKOFFS_MS);

    const err = await settled;
    expect(err).toBeInstanceOf(RequestCancelledError);
    expect(err).not.toBeInstanceOf(ApiError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
