// Tests for the same-origin SSE proxy route handler at
// app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/stream.
//
// The handler is the trust boundary between the browser (cookie-authed via
// Clerk) and the Zig backend (Bearer-only, aud=api.agentsfleet.net). Coverage
// here pins the auth + error + stream-piping contract documented in
// docs/AUTH.md "UI · SSE stream".

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

import { GET } from "../app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/stream/route";

function makeReq(): Request {
  return new Request("http://localhost/proxy", { method: "GET" });
}

function paramsOf(workspaceId: string, fleetId: string) {
  return { params: Promise.resolve({ workspaceId, fleetId }) };
}

describe("SSE route handler — auth", () => {
  it("returns 401 with the registered 401 code when Clerk has no session token", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(401);
    expect(res.headers.get("content-type")).toBe("application/json");
    const body = (await res.json()) as { error: string; code: string };
    expect(body.code).toBe(ERROR_CODE.AUTH_401);
    expect(body.error).toBe("Unauthorized");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("requests the customized default session token (no template arg)", async () => {
    // Post-Stage-1: the customized default session token carries
    // `aud=https://api.agentsfleet.net` + `metadata.tenant_id`, satisfying
    // agentsfleetd's OIDC verifier without the api-template indirection.
    getTokenFn.mockResolvedValueOnce("session_jwt_token");
    fetchSpy.mockResolvedValueOnce(
      new Response("data: hi\n\n", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      }),
    );
    await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(getTokenFn).toHaveBeenCalledWith();
  });
});

describe("SSE route handler — upstream call", () => {
  it("forwards the Bearer JWT and asks for text/event-stream", async () => {
    getTokenFn.mockResolvedValueOnce("api_jwt_token");
    fetchSpy.mockResolvedValueOnce(
      new Response("data: hi\n\n", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      }),
    );
    await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(
      "https://api.example.test/v1/workspaces/ws_1/fleets/zomb_1/events/stream",
    );
    const headers = (init as RequestInit).headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer api_jwt_token");
    expect(headers.Accept).toBe("text/event-stream");
    expect((init as RequestInit).method).toBe("GET");
  });

  it("URL-encodes path parameters to defend against traversal", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response("", { status: 200, headers: { "content-type": "text/event-stream" } }),
    );
    await GET(makeReq(), paramsOf("ws/../admin", "zomb 1"));
    const [url] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(
      "https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/fleets/zomb%201/events/stream",
    );
  });

  it("propagates the request abort signal to the upstream fetch", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response("", { status: 200, headers: { "content-type": "text/event-stream" } }),
    );
    const ctl = new AbortController();
    const req = new Request("http://localhost/proxy", { method: "GET", signal: ctl.signal });
    await GET(req, paramsOf("ws_1", "zomb_1"));
    const [, init] = fetchSpy.mock.calls[0]!;
    expect((init as RequestInit).signal).toBe(ctl.signal);
  });
});

describe("SSE route handler — happy path streaming", () => {
  it("returns the upstream body with SSE + anti-buffer headers", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response("data: hello\n\n", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      }),
    );
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("text/event-stream");
    expect(res.headers.get("cache-control")).toBe("no-cache, no-transform");
    expect(res.headers.get("connection")).toBe("keep-alive");
    expect(res.headers.get("x-accel-buffering")).toBe("no");
    expect(await res.text()).toBe("data: hello\n\n");
  });
});

describe("SSE route handler — upstream errors", () => {
  it("forwards a non-OK upstream status code with its body", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response("rate limited", {
        status: 429,
        headers: { "content-type": "text/plain" },
      }),
    );
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(429);
    expect(res.headers.get("content-type")).toBe("text/plain");
    expect(await res.text()).toBe("rate limited");
  });

  it("falls back to a synthetic body when upstream has no payload", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(new Response("", { status: 503 }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(503);
    expect(await res.text()).toBe("Upstream error 503");
  });

  it("returns 502 (not status passthrough) when upstream is OK but has no body", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    const noBody = new Response(null, { status: 200, headers: {} });
    fetchSpy.mockResolvedValueOnce(noBody);
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(502);
    expect(res.headers.get("content-type")).toBe("text/plain");
    expect(await res.text()).toBe("Upstream returned no body");
  });

  it("falls back to text/plain when upstream omits content-type on a non-OK response", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(new Response("oops", { status: 500, headers: {} }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(500);
    expect(res.headers.get("content-type")).toMatch(/^text\/plain/);
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

// A member removed while their tab slept is refused at open. An EventSource
// sees only a bare `error` for that and would reconnect forever, so the proxy
// re-says the refusal as the frame that ends the stream.
describe("SSE route handler — refused at open for lost access", () => {
  const FORBIDDEN = 403;
  const PROBLEM_JSON = "application/problem+json";
  const problem = (errorCode: string) => JSON.stringify({ title: "Forbidden", error_code: errorCode });
  // pin test: literal is the contract — the bytes the daemon writes for
  // `Frame::access_revoked` (rustd/crates/afd_sse/src/frame.rs).
  const ACCESS_REVOKED_WIRE =
    'id: 0\nevent: access_revoked\ndata: {"kind":"access_revoked","error_code":"UZ-AUTH-001"}\n\n';

  it("should answer one access_revoked frame as a 200 stream when the daemon refuses with UZ-AUTH-001", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(
      new Response(problem(ERROR_CODE.AUTH_FORBIDDEN), { status: FORBIDDEN, headers: { "content-type": PROBLEM_JSON } }),
    );
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("text/event-stream");
    expect(res.headers.get("cache-control")).toBe("no-cache, no-transform");
    expect(await res.text()).toBe(ACCESS_REVOKED_WIRE);
  });

  it("should forward any other 403 unchanged, so a missing scope is not read as lost access", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    const body = problem(ERROR_CODE.INSUFFICIENT_SCOPE);
    fetchSpy.mockResolvedValueOnce(new Response(body, { status: FORBIDDEN, headers: { "content-type": PROBLEM_JSON } }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(FORBIDDEN);
    expect(res.headers.get("content-type")).toBe(PROBLEM_JSON);
    expect(await res.text()).toBe(body);
  });

  it("should forward a 403 whose body is not the daemon's problem JSON unchanged", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    fetchSpy.mockResolvedValueOnce(new Response("forbidden by proxy", { status: FORBIDDEN, headers: { "content-type": "text/html" } }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(FORBIDDEN);
    expect(res.headers.get("content-type")).toBe("text/html");
    expect(await res.text()).toBe("forbidden by proxy");
  });

  it("should forward a 500 that carries UZ-AUTH-001 unchanged, since only a 403 is a refusal", async () => {
    getTokenFn.mockResolvedValueOnce("tk");
    const body = problem(ERROR_CODE.AUTH_FORBIDDEN);
    fetchSpy.mockResolvedValueOnce(new Response(body, { status: 500, headers: { "content-type": PROBLEM_JSON } }));
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(500);
    expect(await res.text()).toBe(body);
  });
});
