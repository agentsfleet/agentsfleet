import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, RequestCancelledError } from "./errors";
import { request } from "./client";
import { RETRY_CODE_TIMEOUT, RETRY_DEFAULTS } from "./retry";
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

  it("a caller signal wins over the default timeout", async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, OK_BODY));
    const controller = new AbortController();

    await request(PATH, { method: "GET", signal: controller.signal }, TOKEN);
    await request(PATH, { method: "GET" }, TOKEN);

    // The caller's own signal is passed through untouched…
    expect(sentSignal(0)).toBe(controller.signal);
    // …and a request with none is still given one, so no fetch is unbounded.
    const defaulted = sentSignal(1);
    expect(defaulted).toBeInstanceOf(AbortSignal);
    expect(defaulted).not.toBe(controller.signal);
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
