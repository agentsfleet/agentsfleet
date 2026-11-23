import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, RETRY_CODE_TIMEOUT, RequestCancelledError } from "./errors";
import { request } from "./client";
import {
  readWorkspaceFetchAudit,
  resetWorkspaceFetchAudit,
  WORKSPACE_LIST_PATH,
} from "../acceptance/workspace-fetch-audit";

// What `request()` does with NO options: one attempt, the caller's signal and
// no other budget, and every interrupted read classified as what it was.

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

const PATH = "/v1/thing";
const TOKEN = "tok";
const OK_BODY = { ok: 1 };
const TRANSIENT_STATUS = 503;

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
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  resetWorkspaceFetchAudit();
  fetchMock.mockReset();
});

describe("request — one attempt", () => {
  it("a transient read fails with its status after one attempt", async () => {
    fetchMock.mockResolvedValue(jsonResponse(TRANSIENT_STATUS, { detail: "svc" }));

    const err = (await request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e)) as ApiError;

    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(TRANSIENT_STATUS);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("the acceptance audit counts a read once", async () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    fetchMock.mockResolvedValue(jsonResponse(200, { items: [] }));
    await request(WORKSPACE_LIST_PATH, { method: "GET" }, TOKEN);
    expect(readWorkspaceFetchAudit()).toEqual({ total: 1, byPath: { [WORKSPACE_LIST_PATH]: 1 } });
  });
});

describe("request — signals", () => {
  it("passes the caller's signal through and mints no timer of its own", async () => {
    fetchMock.mockResolvedValue(jsonResponse(200, OK_BODY));
    const controller = new AbortController();
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout");

    await request(PATH, { method: "GET", signal: controller.signal }, TOKEN);
    await request(PATH, { method: "GET" }, TOKEN);

    expect(sentSignal(0)).toBe(controller.signal);
    expect(sentSignal(1)).toBeUndefined();
    expect(timeoutSpy).not.toHaveBeenCalled();
  });

  it("a timeout during the body read is a timeout, never a success body", async () => {
    fetchMock.mockResolvedValue(bodyRejecting(200, timeoutRejection()));

    const err = (await request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e)) as ApiError;

    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe(RETRY_CODE_TIMEOUT);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("a cancel during the body read is a cancel, never a success body", async () => {
    const controller = new AbortController();
    fetchMock.mockResolvedValue(bodyRejecting(200, abortRejection()));

    const settled = await request(PATH, { method: "GET", signal: controller.signal }, TOKEN).catch(
      (e: unknown) => e,
    );

    expect(settled).toBeInstanceOf(RequestCancelledError);
  });

  it("a navigation abort is a cancel", async () => {
    fetchMock.mockRejectedValue(abortRejection());

    const err = await request(PATH, { method: "GET" }, TOKEN).catch((e: unknown) => e);

    expect(err).toBeInstanceOf(RequestCancelledError);
    expect(err).not.toBeInstanceOf(ApiError);
  });
});
