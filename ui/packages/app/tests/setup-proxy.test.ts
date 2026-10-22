// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { matchSetupRoute } from "@/lib/api/setup-proxy/path";
import { SETUP_BODY_LIMIT_BYTES } from "@/lib/api/setup-proxy/body";
import { SETUP_PROXY_DETAIL, SETUP_REQUEST_ID_HEADER } from "@/lib/api/setup-proxy/response";
import { APP_ORIGIN, DRAFT_ID, FLEET_ID, LIBRARY_ID, REVISION_ID, WORKSPACE, response } from "./helpers/draft-fixtures";

const {credential} = vi.hoisted(() => ({credential: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential}));
import { proxySetup } from "@/lib/api/setup-proxy";
import * as handlers from "@/app/live/v1/workspaces/[workspaceId]/[...setupPath]/route";

const BASE = `/live/v1/workspaces/${WORKSPACE}`;
const METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const upstream = vi.fn();
const REQUEST_ID = "upstream-request";

beforeEach(() => {vi.resetAllMocks(); credential.mockResolvedValue("synthetic-session"); vi.stubGlobal("fetch", upstream);});
afterEach(() => {vi.useRealTimers(); vi.unstubAllGlobals();});

function proxy(parts: string[], init: RequestInit = {}, query = "") {
  const method = init.method ?? METHOD.get;
  const request = new Request(`${APP_ORIGIN}${BASE}/${parts.join("/")}${query}`, {method,
    ...(method === METHOD.get || method === METHOD.delete ? {} : {body: "{}"}), ...init,
    headers: {Origin: APP_ORIGIN, "Content-Type": "application/json", ...init.headers}});
  const handler = {GET: handlers.GET, POST: handlers.POST, PATCH: handlers.PATCH, DELETE: handlers.DELETE}[method] ?? handlers.GET;
  return handler(request, {params: Promise.resolve({workspaceId: WORKSPACE, setupPath: parts})});
}

describe("private setup proxy", () => {
  it.each([undefined, "null", "https://foreign.test", "bad-url"])("rejects a foreign or missing mutation origin %s before minting", async (origin) => {
    const headers = new Headers({"Content-Type": "application/json"});
    if (origin !== undefined) headers.set("Origin", origin);
    const request = new Request(`${APP_ORIGIN}${BASE}/drafts`, {method: METHOD.post, body: "{}", headers});
    expect((await proxySetup(request, {params: Promise.resolve({workspaceId: WORKSPACE, setupPath: ["drafts"]})})).status).toBe(403);
    expect(credential).not.toHaveBeenCalled(); expect(upstream).not.toHaveBeenCalled();
  });
  it("uses the forwarded host and protocol for same-origin writes", async () => {
    upstream.mockResolvedValue(response({accepted: true}));
    const result = await proxy(["drafts"], {method: METHOD.post, headers: {Host: "internal:3000", "X-Forwarded-Host": "app.example.test, edge.test", "X-Forwarded-Proto": "https, http"}});
    expect(result.status).toBe(200); expect(credential).toHaveBeenCalledTimes(1);
  });
  it("refuses an unlisted route or unsupported method before credential access", async () => {
    expect((await proxy(["secrets"], {method: METHOD.post})).status).toBe(404);
    expect((await proxy(["drafts", DRAFT_ID, "review"])).status).toBe(404);
    expect((await proxy(["drafts"], {method: "PUT"})).status).toBe(404);
    expect(credential).not.toHaveBeenCalled();
  });
  it("refuses a non-JSON or oversized mutation before minting", async () => {
    expect((await proxy(["drafts"], {method: METHOD.post, headers: {"Content-Type": "text/plain"}})).status).toBe(415);
    expect((await proxy(["drafts"], {method: METHOD.post, headers: {"Content-Length": String(SETUP_BODY_LIMIT_BYTES + 1)}})).status).toBe(413);
    expect((await proxy(["drafts"], {method: METHOD.post, body: "x".repeat(SETUP_BODY_LIMIT_BYTES + 1)})).status).toBe(413);
    expect(credential).not.toHaveBeenCalled();
  });
  it("forwards a schedule sync retry with no body and no JSON media type", async () => {
    upstream.mockResolvedValue(response({schedule_id: REVISION_ID, sync: "synced"}));
    const parts = ["fleets", FLEET_ID, "schedules", REVISION_ID, "sync"];
    const request = new Request(`${APP_ORIGIN}${BASE}/${parts.join("/")}`, {method: METHOD.post, headers: {Origin: APP_ORIGIN}});
    const result = await handlers.POST(request, {params: Promise.resolve({workspaceId: WORKSPACE, setupPath: parts})});
    expect(result.status).toBe(200);
    const forwarded = upstream.mock.calls[0]?.[1];
    expect(forwarded.body).toBeUndefined(); expect(forwarded.headers.get("Content-Type")).toBeNull();
  });
  it("returns private unauthorized state without forwarding when the session expired", async () => {
    credential.mockResolvedValue(null);
    const result = await proxy(["models"]);
    expect(result.status).toBe(401); expect(result.headers.get("cache-control")).toBe("private, no-store"); expect(upstream).not.toHaveBeenCalled();
  });
  it("forwards exact bytes, mutation keys and only safe presentation headers", async () => {
    upstream.mockResolvedValue(response({detail: "Review changed", review_digest: "fresh"}, 409,
      {ETag: '"current"', Location: "/safe", "Retry-After": "1", [SETUP_REQUEST_ID_HEADER]: REQUEST_ID, "Set-Cookie": "never-forward", Authorization: "never-forward"}));
    const result = await proxy(["fleets"], {method: METHOD.post, body: '{"private":"text"}',
      headers: {"Idempotency-Key": "same-operation", "If-Match": '"setup"', "X-Not-Forwarded": "private"}});
    expect(result.status).toBe(409); expect(await result.json()).toEqual({detail: "Review changed", review_digest: "fresh"});
    expect(result.headers.get("etag")).toBe('"current"'); expect(result.headers.get("set-cookie")).toBeNull(); expect(result.headers.get("authorization")).toBeNull();
    expect(result.headers.get(SETUP_REQUEST_ID_HEADER)).toBe(REQUEST_ID);
    const request = upstream.mock.calls[0]?.[1];
    expect(new TextDecoder().decode(request.body)).toBe('{"private":"text"}');
    expect(request.headers.get("Idempotency-Key")).toBe("same-operation"); expect(request.headers.get("If-Match")).toBe('"setup"');
    expect(request.headers.get("Authorization")).toBe("Bearer synthetic-session"); expect(request.headers.get("X-Not-Forwarded")).toBeNull();
    expect(request.signal).toBeInstanceOf(AbortSignal);
  });
  it("whitelists list parameters and omits arbitrary upstream queries", async () => {
    upstream.mockResolvedValue(response({items: []}));
    await proxy(["models"], {}, "?purpose=authoring&limit=10&starting_after=cursor&credential=secret");
    const target = new URL(upstream.mock.calls[0]?.[0]);
    expect(target.pathname).toBe(`/v1/workspaces/${WORKSPACE}/models`);
    expect(target.search).toBe("?limit=10&starting_after=cursor&purpose=authoring");
    await proxy(["models"]);
    expect(new URL(upstream.mock.calls.at(-1)?.[0]).search).toBe("");
  });
  it("preserves the literal private-chat query together with its bound cursor", async () => {
    upstream.mockResolvedValue(response({items: [], next_cursor: null}));
    const query = new URLSearchParams({q: "build_% failures", limit: "50", starting_after: "opaque+cursor", credential: "discard"});
    await proxy(["drafts"], {}, `?${query}`);
    const target = new URL(upstream.mock.calls[0]?.[0]);
    expect(target.pathname).toBe(`/v1/workspaces/${WORKSPACE}/drafts`);
    expect(target.searchParams.get("q")).toBe("build_% failures");
    expect(target.searchParams.get("starting_after")).toBe("opaque+cursor");
    expect(target.searchParams.has("credential")).toBe(false);
  });
  it("streams progress without buffering and strips content types outside the allowed set", async () => {
    upstream.mockResolvedValueOnce(new Response("event: message.running\ndata: {}\n\n", {headers: {"Content-Type": "text/event-stream"}}));
    const stream = await proxy(["drafts", DRAFT_ID, "events", "stream"]);
    expect(stream.headers.get("x-accel-buffering")).toBe("no"); expect(stream.headers.get("vary")).toBe("Cookie");
    expect(await stream.text()).toContain("message.running");
    upstream.mockResolvedValueOnce(new Response("unexpected", {headers: {"Content-Type": "text/html"}}));
    expect((await proxy(["drafts", DRAFT_ID])).headers.get("content-type")).toBe("text/plain");
  });
  it("supports empty delete responses and keeps uncertain upstream failure distinct", async () => {
    upstream.mockResolvedValueOnce(response(null, 204)).mockRejectedValueOnce(new Error("unconfirmed"));
    expect((await proxy(["drafts", DRAFT_ID], {method: METHOD.delete})).status).toBe(204);
    const result = await proxy(["drafts"], {method: METHOD.post});
    expect(result.status).toBe(502); expect(await result.json()).toEqual({detail: SETUP_PROXY_DETAIL.unconfirmed});
  });
  it("bounds the handshake without timing out an established private stream", async () => {
    vi.useFakeTimers();
    const owner = new AbortController();
    upstream.mockResolvedValue(new Response("event: message.running\ndata: {}\n\n", {headers: {"Content-Type": "text/event-stream"}}));
    const stream = await proxy(["drafts", DRAFT_ID, "events", "stream"], {signal: owner.signal});
    const signal: AbortSignal = upstream.mock.calls[0]?.[1].signal;
    await vi.advanceTimersByTimeAsync(31_000);
    expect(signal.aborted).toBe(false); expect(await stream.text()).toContain("message.running");
    owner.abort(); expect(signal.aborted).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });
  it("rotates an established private stream before the hosting limit and cancels upstream", async () => {
    vi.useFakeTimers();
    const cancel = vi.fn();
    const bytes = new TextEncoder().encode("id: 7\nevent: message.running\ndata: {}\n\n");
    upstream.mockResolvedValueOnce(new Response(new ReadableStream({
      start(controller) { controller.enqueue(bytes); }, cancel,
    }), {headers: {"Content-Type": "text/event-stream"}}));
    const stream = await proxy(["drafts", DRAFT_ID, "events", "stream"]);
    const signal: AbortSignal = upstream.mock.calls[0]?.[1].signal;
    const reader = stream.body!.getReader();
    expect(await reader.read()).toEqual({done: false, value: bytes});
    await vi.advanceTimersByTimeAsync(240_000);
    expect(signal.aborted).toBe(true);
    expect(await reader.read()).toEqual({done: true, value: undefined});
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });
  it("aborts a private stream whose upstream handshake never finishes", async () => {
    vi.useFakeTimers();
    upstream.mockImplementation((_target: string, init: RequestInit) => new Promise((_resolve, reject) => {
      init.signal?.addEventListener("abort", () => reject(init.signal?.reason), {once: true});
    }));
    const pending = proxy(["drafts", DRAFT_ID, "events", "stream"]);
    await vi.advanceTimersByTimeAsync(30_000);
    expect((await pending).status).toBe(502); expect(vi.getTimerCount()).toBe(0);
  });
});

describe("setup route whitelist", () => {
  const allowed = [
    [["models"], METHOD.get], [["drafts"], METHOD.get], [["drafts"], METHOD.post],
    [["drafts", DRAFT_ID], METHOD.get], [["drafts", DRAFT_ID], METHOD.patch], [["drafts", DRAFT_ID], METHOD.delete],
    [["drafts", DRAFT_ID, "messages"], METHOD.get], [["drafts", DRAFT_ID, "messages"], METHOD.post],
    [["drafts", DRAFT_ID, "cancellations"], METHOD.post], [["drafts", DRAFT_ID, "events", "stream"], METHOD.get],
    [["fleets"], METHOD.get], [["fleets"], METHOD.post], [["fleets", FLEET_ID], METHOD.get], [["fleets", FLEET_ID], METHOD.patch], [["fleets", FLEET_ID, "revisions"], METHOD.post],
    [["fleets", FLEET_ID, "schedules", REVISION_ID], METHOD.get], [["fleets", FLEET_ID, "schedules", REVISION_ID], METHOD.patch],
    [["fleets", FLEET_ID, "schedules", REVISION_ID, "sync"], METHOD.post],
    [["fleet-libraries"], METHOD.get], [["fleet-libraries"], METHOD.post], [["fleet-libraries", "owned"], METHOD.get],
    [["fleet-libraries", LIBRARY_ID], METHOD.get], [["fleet-libraries", "platform-recipe"], METHOD.get], [["fleet-libraries", LIBRARY_ID], METHOD.delete],
    [["fleet-libraries", LIBRARY_ID, "revisions"], METHOD.get], [["fleet-libraries", LIBRARY_ID, "revisions"], METHOD.post], [["fleet-libraries", LIBRARY_ID, "revisions", REVISION_ID], METHOD.get],
  ] as const;
  it.each(allowed)("matches %s %s", (path, method) => {expect(matchSetupRoute(WORKSPACE, path, method)).toBeDefined();});
  it.each([
    ["models", DRAFT_ID], ["drafts", ".."], ["drafts", DRAFT_ID, "messages", "extra"], ["fleets", FLEET_ID, "secrets"],
    ["fleet-libraries", "owned", "revisions"], ["fleet-libraries", ".."], ["fleet-libraries", "x".repeat(129)],
    ["fleet-libraries", LIBRARY_ID, "revisions", "invalid"], ["fleet-libraries", LIBRARY_ID, "other"],
  ])("rejects unsupported path %s", (...path) => {expect(matchSetupRoute(WORKSPACE, path, METHOD.get)).toBeUndefined();});
  it("rejects a malformed workspace", () => {expect(matchSetupRoute("..", ["models"], METHOD.get)).toBeUndefined();});
  it.each([
    [["fleets"], METHOD.patch], [["fleets", "invalid"], METHOD.get],
    [["fleets", FLEET_ID, "schedules", REVISION_ID], METHOD.delete],
    [["fleets", FLEET_ID, "schedules", "invalid"], METHOD.get],
    [["fleets", FLEET_ID, "schedules", REVISION_ID, "sync"], METHOD.get],
    [["fleets", FLEET_ID, "schedules", "invalid", "sync"], METHOD.post],
    [["fleets", FLEET_ID, "schedules", REVISION_ID, "other"], METHOD.post],
    [["drafts", DRAFT_ID, "messages"], METHOD.patch],
    [["fleet-libraries"], METHOD.patch], [["fleet-libraries", LIBRARY_ID], METHOD.patch],
    [["fleet-libraries", ""], METHOD.get], [["fleet-libraries", LIBRARY_ID, "revisions"], METHOD.patch],
    [["fleet-libraries", LIBRARY_ID, "revisions", REVISION_ID], METHOD.post],
  ] as const)("rejects an unsupported method/path pair %s %s", (path, method) => {
    expect(matchSetupRoute(WORKSPACE, path, method)).toBeUndefined();
  });
});
