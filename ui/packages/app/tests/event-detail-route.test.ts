// Tests for the same-origin event detail proxy route handler at
// app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId].
//
// The one-event sibling of backfill-route.test.ts: same Clerk trust boundary
// (cookie-authed browser → Bearer-only backend), but one event's saved row,
// streamed through. Coverage pins the auth, path guarding and
// error-passthrough behavior the chat's stall and replay reads depend on.

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

import { GET } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/route";

const WS = "ws_1";
const FLEET = "fleet_1";
const TOKEN = "tk";
const HEADER_CONTENT_TYPE = "content-type";
const HEADER_CACHE_CONTROL = "cache-control";
const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";
const NO_STORE = "no-store";
const EVENT_ID = "1790573387481-566";
const DOT_DOT = "..";
const NOT_FOUND_BODY = JSON.stringify({ error: "not found" });
const UPSTREAM_DETAIL = JSON.stringify({ event_id: EVENT_ID, status: "processed", response_text: "Done." });

function makeReq(signal?: AbortSignal): Request {
  return new Request("http://localhost/proxy", { method: "GET", signal });
}

function paramsOf(workspaceId: string, fleetId: string, eventId: string) {
  return { params: Promise.resolve({ workspaceId, fleetId, eventId }) };
}

function upstreamDetail(): Response {
  return new Response(UPSTREAM_DETAIL, { status: 200, headers: { [HEADER_CONTENT_TYPE]: CONTENT_TYPE_JSON } });
}

describe("event detail route handler — auth", () => {
  it("test_event_detail_route_unauthorized — 401 with the registered auth code and no upstream call when Clerk has no session token", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(401);
    expect(res.headers.get(HEADER_CONTENT_TYPE)).toBe(CONTENT_TYPE_JSON);
    expect(res.headers.get(HEADER_CACHE_CONTROL)).toBe(NO_STORE);
    const body = (await res.json()) as { error: string; code: string };
    expect(body.code).toBe(ERROR_CODE.AUTH_401);
    expect(body.error).toBe("Unauthorized");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("rejects a dot-only segment in any of the three params before minting or calling upstream", async () => {
    // encodeURIComponent leaves '.' intact; a bare '..' segment would
    // dot-normalize inside fetch and steer the token at a different
    // upstream path.
    const cases = [[DOT_DOT, FLEET, EVENT_ID], [WS, DOT_DOT, EVENT_ID], [WS, FLEET, DOT_DOT], [WS, FLEET, "."]] as const;
    for (const [ws, fleet, eventId] of cases) {
      const res = await GET(makeReq(), paramsOf(ws, fleet, eventId));
      expect(res.status).toBe(400);
      expect(res.headers.get(HEADER_CACHE_CONTROL)).toBe(NO_STORE);
    }
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(getTokenFn).not.toHaveBeenCalled();
  });
});

describe("event detail route handler — authed proxying", () => {
  it("test_event_detail_route_proxies_authed — mints the token, reads the one event, and streams its row back", async () => {
    getTokenFn.mockResolvedValueOnce("api_jwt_token");
    fetchSpy.mockResolvedValueOnce(upstreamDetail());
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(getTokenFn).toHaveBeenCalledWith();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(`https://api.example.test/v1/workspaces/${WS}/fleets/${FLEET}/events/${EVENT_ID}`);
    const headers = (init as RequestInit).headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer api_jwt_token");
    expect(headers.Accept).toBe(CONTENT_TYPE_JSON);
    expect(res.status).toBe(200);
    expect(res.headers.get(HEADER_CONTENT_TYPE)).toBe(CONTENT_TYPE_JSON);
    // Authed per-tenant JSON must never land in a shared cache.
    expect(res.headers.get(HEADER_CACHE_CONTROL)).toBe(NO_STORE);
    expect(await res.text()).toBe(UPSTREAM_DETAIL);
  });

  it("URL-encodes path parameters to defend against traversal", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(upstreamDetail());
    await GET(makeReq(), paramsOf("ws/../admin", "fleet 1", "evt?x=1"));
    const [url] = fetchSpy.mock.calls[0]!;
    expect(url).toBe("https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/fleets/fleet%201/events/evt%3Fx%3D1");
  });

  it("propagates the request abort signal to the upstream fetch", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(upstreamDetail());
    const ctl = new AbortController();
    await GET(makeReq(ctl.signal), paramsOf(WS, FLEET, EVENT_ID));
    const [, init] = fetchSpy.mock.calls[0]!;
    expect((init as RequestInit).signal).toBe(ctl.signal);
  });
});

describe("event detail route handler — upstream errors", () => {
  it("test_event_detail_route_upstream_error_passthrough — a non-2xx upstream passes through with its status and JSON body", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(
      new Response(NOT_FOUND_BODY, { status: 404, headers: { [HEADER_CONTENT_TYPE]: CONTENT_TYPE_JSON } }),
    );
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(404);
    expect(res.headers.get(HEADER_CONTENT_TYPE)).toBe(CONTENT_TYPE_JSON);
    expect(res.headers.get(HEADER_CACHE_CONTROL)).toBe(NO_STORE);
    expect(await res.text()).toBe(NOT_FOUND_BODY);
  });

  it("never reflects an upstream error under a markup-capable content type", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(
      new Response("<script>alert(1)</script>", { status: 500, headers: { [HEADER_CONTENT_TYPE]: "text/html" } }),
    );
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(500);
    expect(res.headers.get(HEADER_CONTENT_TYPE)).toBe(CONTENT_TYPE_TEXT);
  });

  it("falls back to a synthetic body when the upstream error has no payload or type", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(new Response(null, { status: 503 }));
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(503);
    expect(res.headers.get(HEADER_CONTENT_TYPE)).toBe(CONTENT_TYPE_TEXT);
    expect(await res.text()).toBe("Upstream error 503");
  });

  it("returns a pinned 502 envelope when the upstream fetch itself rejects", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockRejectedValueOnce(new Error("connect ECONNREFUSED"));
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(502);
    expect(res.headers.get(HEADER_CACHE_CONTROL)).toBe(NO_STORE);
    const body = (await res.json()) as { error: string };
    expect(body.error).toBe("Upstream unreachable");
  });

  it("survives upstream.text() rejection without throwing", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    const broken = new Response("ignored", { status: 502 });
    Object.defineProperty(broken, "text", {
      value: () => Promise.reject(new Error("read failed")),
    });
    fetchSpy.mockResolvedValueOnce(broken);
    const res = await GET(makeReq(), paramsOf(WS, FLEET, EVENT_ID));
    expect(res.status).toBe(502);
    expect(await res.text()).toBe("Upstream error 502");
  });
});
