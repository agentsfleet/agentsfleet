import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HTTP_STATUS_UNAUTHORIZED } from "./errors";
import { postSteer } from "./fleet-steer";
import { ERROR_CODE } from "@/lib/errors";

const WORKSPACE = "ws/1";
const FLEET = "fleet 1";
const MESSAGE = "deploy the canary";
const OPERATION_ID = "0190a3b4-0000-7000-8000-000000000001";
// pin test: literal is the contract — the route the browser POSTs to, ids encoded.
const ROUTE = "/live/v1/workspaces/ws%2F1/fleets/fleet%201/messages";
const ACCEPTED = { status: "accepted", event_id: "1790573387481-566", replayed: false };
const HTTP_ACCEPTED = 202;
const HTTP_CONFLICT = 409;
const HTTP_BAD_GATEWAY = 502;

const fetchMock = vi.fn();

function answer(status: number, body: unknown, contentType = "application/json"): Response {
  const text = typeof body === "string" ? body : JSON.stringify(body);
  return new Response(text, { status, statusText: `status ${status}`, headers: { "Content-Type": contentType } });
}

function steer(signal = new AbortController().signal) {
  return postSteer(WORKSPACE, FLEET, MESSAGE, OPERATION_ID, signal);
}

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
});

describe("postSteer", () => {
  it("POSTs one JSON steer to the same-origin route under the caller's signal, and reads the 202", async () => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_ACCEPTED, ACCEPTED));
    const signal = new AbortController().signal;
    expect(await steer(signal)).toEqual({ ok: true, data: ACCEPTED });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(ROUTE);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(typeof init.body).toBe("string");
    expect(JSON.parse(init.body as string)).toEqual({ message: MESSAGE, operation_id: OPERATION_ID });
    expect(init.signal).toBe(signal);
    expect(init.redirect).toBe("manual");
    expect(init.cache).toBe("no-store");
  });

  it("carries a refusal's status and code", async () => {
    fetchMock.mockResolvedValueOnce(
      answer(HTTP_CONFLICT, { detail: "conflict", error_code: ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT }, "application/problem+json"),
    );
    expect(await steer()).toEqual({
      ok: false, error: "conflict", status: HTTP_CONFLICT, errorCode: ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT,
    });
  });

  it("keeps a refusal's status when its body is not the problem shape", async () => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_BAD_GATEWAY, "<html>bad gateway</html>", "text/html"));
    expect(await steer()).toEqual({ ok: false, error: `status ${HTTP_BAD_GATEWAY}`, status: HTTP_BAD_GATEWAY, errorCode: undefined });
    fetchMock.mockResolvedValueOnce(answer(HTTP_CONFLICT, { detail: 7 }));
    expect(await steer()).toEqual({ ok: false, error: `status ${HTTP_CONFLICT}`, status: HTTP_CONFLICT, errorCode: undefined });
  });

  it("reads a 202 from a daemon older than the replay field as a fresh admission", async () => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_ACCEPTED, { status: ACCEPTED.status, event_id: ACCEPTED.event_id }));
    expect(await steer()).toEqual({ ok: true, data: ACCEPTED });
  });

  it("reads a success without a receipt as no answer: the daemon may hold the message", async () => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_ACCEPTED, { status: "accepted" }));
    const result = await steer();
    expect(result.ok).toBe(false);
    expect(result).not.toHaveProperty("status");
  });

  it("reads the session check's redirect to sign-in as a 401", async () => {
    const redirected = answer(HTTP_ACCEPTED, "");
    Object.defineProperty(redirected, "type", { value: "opaqueredirect" });
    fetchMock.mockResolvedValueOnce(redirected);
    expect(await steer()).toMatchObject({ ok: false, status: HTTP_STATUS_UNAUTHORIZED, errorCode: ERROR_CODE.AUTH_401 });
  });

  it("answers an abort or a dead socket with no status, never a throw", async () => {
    fetchMock.mockRejectedValueOnce(new DOMException("aborted", "AbortError"));
    const result = await steer();
    expect(result.ok).toBe(false);
    expect(result).not.toHaveProperty("status");
  });
});
