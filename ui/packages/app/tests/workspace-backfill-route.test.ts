// Workspace backfill proxy coverage. The browser calls this same-origin route
// so its Clerk session can become the Bearer token required by agentsfleetd.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { getTokenFn } = vi.hoisted(() => ({ getTokenFn: vi.fn() }));

vi.mock("@clerk/nextjs/server", () => ({
  auth: () => Promise.resolve({ getToken: getTokenFn }),
}));

vi.mock("@/lib/api/client", () => ({
  API_ORIGIN: "https://api.example.test",
}));

import { GET } from "../app/backend/v1/workspaces/[workspaceId]/events/route";

const WORKSPACE_ID = "ws_1";
const TOKEN = "session_token";
const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";
const STATUS_BAD_REQUEST = 400;
const STATUS_UNAUTHORIZED = 401;
const STATUS_BAD_GATEWAY = 502;
const STATUS_UNAVAILABLE = 503;
const PAGE_LIMIT = 200;
const UPSTREAM_PAGE = '{"items":[],"next_cursor":null}';

const fetchSpy = vi.fn();
const originalFetch = globalThis.fetch;

beforeEach(() => {
  vi.clearAllMocks();
  globalThis.fetch = fetchSpy as unknown as typeof fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

function makeReq(query = ""): Request {
  return new Request(`http://localhost/proxy${query}`, { method: "GET" });
}

function paramsOf(workspaceId = WORKSPACE_ID) {
  return { params: Promise.resolve({ workspaceId }) };
}

describe("workspace backfill route", () => {
  it("rejects dot-only workspace identifiers before minting a token", async () => {
    const res = await GET(makeReq(), paramsOf(".."));

    expect(res.status).toBe(STATUS_BAD_REQUEST);
    expect(res.headers.get("cache-control")).toBe("no-store");
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

  it("encodes the path, forwards only allowed queries, and returns the bounded page", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(
      new Response(UPSTREAM_PAGE, {
        status: 200,
        headers: { "content-type": CONTENT_TYPE_JSON },
      }),
    );
    const controller = new AbortController();
    const req = new Request(
      `http://localhost/proxy?cursor=next&since=12&limit=${PAGE_LIMIT}&fleet_id=fleet_a&ignored=x`,
      { signal: controller.signal },
    );

    const res = await GET(req, paramsOf("ws/../admin"));

    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(url).toBe(
      `https://api.example.test/v1/workspaces/ws%2F..%2Fadmin/events?cursor=next&since=12&limit=${PAGE_LIMIT}&fleet_id=fleet_a`,
    );
    expect((init as RequestInit).headers).toEqual({
      Authorization: `Bearer ${TOKEN}`,
      Accept: CONTENT_TYPE_JSON,
    });
    expect((init as RequestInit).signal).toBe(controller.signal);
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(await res.text()).toBe(UPSTREAM_PAGE);
  });

  it("omits the query delimiter when no allowed query is present", async () => {
    getTokenFn.mockResolvedValueOnce(TOKEN);
    fetchSpy.mockResolvedValueOnce(new Response(UPSTREAM_PAGE, { status: 200 }));

    await GET(makeReq("?ignored=x"), paramsOf());

    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      `https://api.example.test/v1/workspaces/${WORKSPACE_ID}/events`,
    );
  });

  it("preserves JSON upstream errors and normalizes non-JSON errors to text", async () => {
    getTokenFn.mockResolvedValue(TOKEN);
    fetchSpy
      .mockResolvedValueOnce(
        new Response('{"error":"unavailable"}', {
          status: STATUS_UNAVAILABLE,
          headers: { "content-type": `${CONTENT_TYPE_JSON}; charset=utf-8` },
        }),
      )
      .mockResolvedValueOnce(
        new Response("unavailable", {
          status: STATUS_UNAVAILABLE,
          headers: { "content-type": "application/problem+json" },
        }),
      );

    const jsonRes = await GET(makeReq(), paramsOf());
    const textRes = await GET(makeReq(), paramsOf());

    expect(jsonRes.headers.get("content-type")).toBe(`${CONTENT_TYPE_JSON}; charset=utf-8`);
    expect(textRes.headers.get("content-type")).toBe(CONTENT_TYPE_TEXT);
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
    await expect(res.json()).resolves.toEqual({ error: "Upstream unreachable" });
  });
});
