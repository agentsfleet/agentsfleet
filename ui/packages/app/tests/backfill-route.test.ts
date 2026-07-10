// Tests for the same-origin backfill proxy route handler at
// app/backend/v1/workspaces/[workspaceId]/fleets/[fleetId]/events.
//
// The non-stream sibling of sse-route.test.ts: same Clerk trust boundary
// (cookie-authed browser → Bearer-only Zig backend), but a buffered JSON
// events page instead of a piped stream. Coverage pins the auth, query
// forwarding, and error-passthrough behavior the reconnect backfill
// (fleet-stream-registry) depends on.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

import { GET } from "../app/backend/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/route";

const UPSTREAM_PAGE = JSON.stringify({
  items: [{ event_id: "evt_1" }],
  next_cursor: null,
});

function makeReq(query = ""): Request {
  return new Request(`http://localhost/proxy${query}`, { method: "GET" });
}

function paramsOf(workspaceId: string, fleetId: string) {
  return { params: Promise.resolve({ workspaceId, fleetId }) };
}

describe("backfill route handler — auth", () => {
  it("test_backfill_route_unauthorized — 401 with UZ-401 body and no upstream call when Clerk has no session token", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(401);
    expect(res.headers.get("content-type")).toBe("application/json");
    expect(res.headers.get("cache-control")).toBe("no-store");
    const body = (await res.json()) as { error: string; code: string };
    expect(body.code).toBe("UZ-401");
    expect(body.error).toBe("Unauthorized");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("rejects dot-only path segments before minting or calling upstream", async () => {
    // encodeURIComponent leaves '.' intact; a bare '..' segment would
    // dot-normalize inside fetch and steer the token at a different
    // upstream path.
    for (const [ws, fleet] of [["..", "zomb_1"], ["ws_1", ".."], [".", "zomb_1"]] as const) {
      const res = await GET(makeReq(), paramsOf(ws, fleet));
      expect(res.status).toBe(400);
    }
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(getTokenFn).not.toHaveBeenCalled();
  });
});

describe("backfill route handler — authed proxying", () => {
  it("test_backfill_route_proxies_authed — mints the token, forwards the bounded query, and returns the upstream page", async () => {
    getTokenFn.mockResolvedValueOnce("api_jwt_token");
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const res = await GET(
      makeReq("?since=2026-05-15T18%3A29%3A58Z&limit=200&actor=dropped"),
      paramsOf("ws_1", "zomb_1"),
    );
    expect(getTokenFn).toHaveBeenCalledWith();
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0]!;
    // cursor/since/limit forward; the non-allowlisted `actor` is dropped.
    expect(url).toBe(
      "https://api.example.test/v1/workspaces/ws_1/fleets/zomb_1/events" +
        "?since=2026-05-15T18%3A29%3A58Z&limit=200",
    );
    const headers = (init as RequestInit).headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer api_jwt_token");
    expect(headers.Accept).toBe("application/json");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("application/json");
    // Authed per-tenant JSON must never land in a shared cache.
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(await res.text()).toBe(UPSTREAM_PAGE);
  });

  it("forwards a cursor through the allowlist", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    await GET(makeReq("?cursor=abc123"), paramsOf("ws_1", "zomb_1"));
    const [url] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(
      "https://api.example.test/v1/workspaces/ws_1/fleets/zomb_1/events?cursor=abc123",
    );
  });

  it("omits the query string entirely when the caller sends no forwardable keys", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    const [url] = fetchSpy.mock.calls[0]!;
    expect(url).toBe("https://api.example.test/v1/workspaces/ws_1/fleets/zomb_1/events");
  });

  it("URL-encodes path parameters to defend against traversal", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    await GET(makeReq(), paramsOf("ws/../admin", "zomb 1"));
    const [url] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(
      "https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/fleets/zomb%201/events",
    );
  });

  it("propagates the request abort signal to the upstream fetch", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const ctl = new AbortController();
    const req = new Request("http://localhost/proxy", { method: "GET", signal: ctl.signal });
    await GET(req, paramsOf("ws_1", "zomb_1"));
    const [, init] = fetchSpy.mock.calls[0]!;
    expect((init as RequestInit).signal).toBe(ctl.signal);
  });
});

describe("backfill route handler — upstream errors", () => {
  it("test_backfill_route_upstream_error_passthrough — a non-2xx upstream passes through with its status and body", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response('{"error":"unavailable"}', {
        status: 503,
        headers: { "content-type": "application/json" },
      }),
    );
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(503);
    expect(res.headers.get("content-type")).toBe("application/json");
    expect(await res.text()).toBe('{"error":"unavailable"}');
  });

  it("falls back to a synthetic body when the upstream error has no payload", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(new Response("", { status: 500 }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(500);
    expect(await res.text()).toBe("Upstream error 500");
  });

  it("returns a pinned 502 envelope when the upstream fetch itself rejects", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockRejectedValueOnce(new Error("connect ECONNREFUSED"));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(502);
    expect(res.headers.get("cache-control")).toBe("no-store");
    const body = (await res.json()) as { error: string };
    expect(body.error).toBe("Upstream unreachable");
  });

  it("survives upstream.text() rejection without throwing", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    const broken = new Response("ignored", { status: 502 });
    Object.defineProperty(broken, "text", {
      value: () => Promise.reject(new Error("read failed")),
    });
    fetchSpy.mockResolvedValueOnce(broken);
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(502);
    expect(await res.text()).toBe("Upstream error 502");
  });
});
