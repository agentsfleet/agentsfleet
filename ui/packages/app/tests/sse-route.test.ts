// Tests for the same-origin SSE proxy route handler at
// app/backend/v1/workspaces/[workspaceId]/zombies/[zombieId]/events/stream.
//
// The handler is the trust boundary between the browser (cookie-authed via
// Clerk) and the Zig backend (Bearer-only, aud=api.usezombie.com). Coverage
// here pins the auth + error + stream-piping contract documented in
// docs/AUTH.md "UI · SSE stream".

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

import { GET } from "../app/backend/v1/workspaces/[workspaceId]/zombies/[zombieId]/events/stream/route";

function makeReq(): Request {
  return new Request("http://localhost/proxy", { method: "GET" });
}

function paramsOf(workspaceId: string, zombieId: string) {
  return { params: Promise.resolve({ workspaceId, zombieId }) };
}

describe("SSE route handler — auth", () => {
  it("returns 401 with UZ-401 body when Clerk has no session token", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const res = await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(res.status).toBe(401);
    expect(res.headers.get("content-type")).toBe("application/json");
    const body = (await res.json()) as { error: string; code: string };
    expect(body.code).toBe("UZ-401");
    expect(body.error).toBe("Unauthorized");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("requests the api-audience template, not the default JWT", async () => {
    getTokenFn.mockResolvedValueOnce("api_jwt_token");
    fetchSpy.mockResolvedValueOnce(
      new Response("data: hi\n\n", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      }),
    );
    await GET(makeReq(), paramsOf("ws_1", "zomb_1"));
    expect(getTokenFn).toHaveBeenCalledWith({ template: "api" });
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
      "https://api.example.test/v1/workspaces/ws_1/zombies/zomb_1/events/stream",
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
      "https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/zombies/zomb%201/events/stream",
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
