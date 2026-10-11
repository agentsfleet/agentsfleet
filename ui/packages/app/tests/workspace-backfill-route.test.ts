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

import { GET } from "../app/live/v1/workspaces/[workspaceId]/events/route";

const WORKSPACE_ID = "ws_1";
const TOKEN = "session_token";
const CONTENT_TYPE_JSON = "application/json";
const CONTENT_TYPE_TEXT = "text/plain";
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

});
