// Workspace Server-Sent Events (SSE) proxy coverage. The route mints the
// upstream Bearer token and pipes the stream body without buffering it.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { getTokenFn } = vi.hoisted(() => ({ getTokenFn: vi.fn() }));

vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: getTokenFn }),
}));

vi.mock("@/lib/api/client", () => ({
  API_ORIGIN: "https://api.example.test",
}));

import { GET } from "../app/backend/v1/workspaces/[workspaceId]/events/stream/route";

const WORKSPACE_ID = "ws_1";
const TOKEN = "session_token";
const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_STREAM = "text/event-stream";
const CONTENT_TYPE_TEXT = "text/plain";
const STATUS_BAD_REQUEST = 400;
const STATUS_UNAUTHORIZED = 401;
const STATUS_BAD_GATEWAY = 502;
const STATUS_UNAVAILABLE = 503;

const fetchSpy = vi.fn();
const originalFetch = globalThis.fetch;

beforeEach(() => {
  vi.clearAllMocks();
  globalThis.fetch = fetchSpy as unknown as typeof fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

function makeReq(): Request {
  return new Request("http://localhost/proxy", { method: "GET" });
}

function paramsOf(workspaceId = WORKSPACE_ID) {
  return { params: Promise.resolve({ workspaceId }) };
}

describe("workspace SSE route", () => {
  it("rejects dot-only workspace identifiers before minting a token", async () => {
    const res = await GET(makeReq(), paramsOf("."));

    expect(res.status).toBe(STATUS_BAD_REQUEST);
    expect(getTokenFn).not.toHaveBeenCalled();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("returns the pinned unauthorized envelope when no session token exists", async () => {
    getTokenFn.mockResolvedValueOnce(null);

    const res = await GET(makeReq(), paramsOf());

    expect(res.status).toBe(STATUS_UNAUTHORIZED);
    await expect(res.json()).resolves.toEqual({ error: "Unauthorized", code: "UZ-401" });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("encodes the path, forwards authorization and cancellation, then pipes the stream", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(
      new Response("data: hello\n\n", {
        status: 200,
        headers: { "content-type": CONTENT_TYPE_STREAM },
      }),
    );
    const controller = new AbortController();
    const req = new Request("http://localhost/proxy", { signal: controller.signal });

    const res = await GET(req, paramsOf("ws/../admin"));

    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(url).toBe("https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/events/stream");
    expect((init as RequestInit).headers).toEqual({
      Authorization: `Bearer ${TOKEN}`,
      Accept: CONTENT_TYPE_STREAM,
    });
    expect((init as RequestInit).signal).toBe(controller.signal);
    expect(getTokenFn).toHaveBeenCalledWith();
    expect(res.headers.get("content-type")).toBe(CONTENT_TYPE_STREAM);
    expect(res.headers.get("cache-control")).toBe("no-cache, no-transform");
    expect(res.headers.get("connection")).toBe("keep-alive");
    expect(res.headers.get("x-accel-buffering")).toBe("no");
    expect(await res.text()).toBe("data: hello\n\n");
  });

  it("passes through an upstream error body and content type", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(
      new Response("unavailable", {
        status: STATUS_UNAVAILABLE,
        headers: { "content-type": CONTENT_TYPE_TEXT },
      }),
    );

    const res = await GET(makeReq(), paramsOf());

    expect(res.status).toBe(STATUS_UNAVAILABLE);
    expect(res.headers.get("content-type")).toBe(CONTENT_TYPE_TEXT);
    expect(await res.text()).toBe("unavailable");
  });

  it("uses a synthetic upstream error when the body and content type are absent", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(new Response(null, { status: STATUS_UNAVAILABLE }));

    const res = await GET(makeReq(), paramsOf());

    expect(res.headers.get("content-type")).toBe(CONTENT_TYPE_TEXT);
    expect(await res.text()).toBe(`Upstream error ${STATUS_UNAVAILABLE}`);
  });

  it("survives rejected upstream body reads", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    const broken = new Response("ignored", { status: STATUS_BAD_GATEWAY });
    Object.defineProperty(broken, "text", {
      value: () => Promise.reject(new Error("read failed")),
    });
    fetchSpy.mockResolvedValueOnce(broken);

    const res = await GET(makeReq(), paramsOf());

    expect(await res.text()).toBe(`Upstream error ${STATUS_BAD_GATEWAY}`);
  });

  it("returns a bounded error when the upstream fetch rejects", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockRejectedValueOnce(new Error("connect failed"));

    const res = await GET(makeReq(), paramsOf());

    expect(res.status).toBe(STATUS_BAD_GATEWAY);
    expect(res.headers.get("content-type")).toBe(CONTENT_TYPE_TEXT);
    expect(await res.text()).toBe("Upstream unreachable");
  });

  it("rejects an upstream success response that has no stream body", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(new Response(null, { status: 200 }));

    const res = await GET(makeReq(), paramsOf());

    expect(res.status).toBe(STATUS_BAD_GATEWAY);
    expect(await res.text()).toBe("Upstream returned no body");
  });
});
