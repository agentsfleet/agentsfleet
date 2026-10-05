// Tests for the same-origin full tool-call read at
// app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/tool-calls/[callId].
//
// The auth, error and streaming behaviour it shares with its siblings is
// pinned in event-detail-route.test.ts; this pins what is its own: the fenced
// call id reaches upstream encoded once, and a 404 (output not kept) reaches
// the dialog unchanged.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ERROR_CODE } from "@/lib/errors";

const { getTokenFn } = vi.hoisted(() => ({ getTokenFn: vi.fn() }));

vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: getTokenFn }),
}));

vi.mock("@/lib/api/client", () => ({
  API_ORIGIN: "https://api.example.test",
  request: vi.fn(),
}));

const fetchSpy = vi.fn();
const originalFetch = globalThis.fetch;

beforeEach(() => {
  vi.clearAllMocks();
  globalThis.fetch = fetchSpy as unknown as typeof fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

import { GET } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/tool-calls/[callId]/route";

const WS = "ws_1";
const FLEET = "fleet_1";
const EVENT_ID = "1790573387481-566";
const CALL_ID = "7:3";
const HTTP_NOT_FOUND = 404;
const CONTENT_TYPE_JSON = "application/json";
const NOT_KEPT_BODY = JSON.stringify({ error_code: "UZ-EVENT-404" });
const FULL_CALL = JSON.stringify({ call_id: CALL_ID, output: "a\nb", output_line_count: 2 });

function call(callId: string): Promise<Response> {
  const req = new Request("http://localhost/proxy", { method: "GET" });
  return GET(req, { params: Promise.resolve({ workspaceId: WS, fleetId: FLEET, eventId: EVENT_ID, callId }) });
}

function upstream(body: string, status: number): Response {
  return new Response(body, { status, headers: { "content-type": CONTENT_TYPE_JSON } });
}

describe("tool call route handler", () => {
  it("test_tool_call_proxy_forwards_encoded_id — the fenced id reaches upstream encoded once and the full call streams back", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(upstream(FULL_CALL, 200));
    const res = await call(CALL_ID);
    const [url] = fetchSpy.mock.calls[0]!;
    // pin test: literal is the contract
    expect(url).toBe(`https://api.example.test/v1/workspaces/${WS}/fleets/${FLEET}/events/${EVENT_ID}/tool-calls/7%3A3`);
    expect(res.status).toBe(200);
    expect(await res.text()).toBe(FULL_CALL);
  });

  it("passes a 404 through with its status and body, so the dialog can say the output was not kept", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(upstream(NOT_KEPT_BODY, HTTP_NOT_FOUND));
    const res = await call(CALL_ID);
    expect(res.status).toBe(HTTP_NOT_FOUND);
    expect(await res.text()).toBe(NOT_KEPT_BODY);
  });

  it("refuses a dot-only call id before minting a token", async () => {
    const res = await call("..");
    expect(res.status).toBe(400);
    expect(getTokenFn).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("answers a signed-out request 401 with the registered code and no upstream call", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await call(CALL_ID);
    expect(res.status).toBe(401);
    expect(((await res.json()) as { code: string }).code).toBe(ERROR_CODE.AUTH_401);
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
