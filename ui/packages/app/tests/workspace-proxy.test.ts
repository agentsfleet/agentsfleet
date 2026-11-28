// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { matchWorkspaceRoute } from "@/lib/api/workspace-proxy/path";
import { CHAT_BODY_LIMIT_BYTES } from "@/lib/api/body-limits";
import { PROXY_DETAIL, PROXY_REQUEST_ID_HEADER } from "@/lib/api/workspace-proxy/response";
import { APP_ORIGIN, FLEET_ID, GRANT_ID, SCHEDULE_ID, TRIGGER_ID, TURN_ID, WORKSPACE, response } from "./helpers/fleet-fixtures";

const {credential} = vi.hoisted(() => ({credential: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential}));
import { proxyWorkspace } from "@/lib/api/workspace-proxy";
import * as handlers from "@/app/live/v1/workspaces/[workspaceId]/[...resourcePath]/route";
import { PATCH as stopTurnRoute } from "@/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/turns/[turnId]/route";
import { PATCH as taskRoute } from "@/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/durable_tasks/[durableTaskId]/route";

const BASE = `/live/v1/workspaces/${WORKSPACE}`;
const METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const upstream = vi.fn();
const REQUEST_ID = "upstream-request";

beforeEach(() => {vi.resetAllMocks(); credential.mockResolvedValue("synthetic-session"); vi.stubGlobal("fetch", upstream);});
afterEach(() => vi.unstubAllGlobals());

function proxy(parts: string[], init: RequestInit = {}, query = "") {
  const method = init.method ?? METHOD.get;
  const request = new Request(`${APP_ORIGIN}${BASE}/${parts.join("/")}${query}`, {method,
    ...(method === METHOD.get || method === METHOD.delete ? {} : {body: "{}"}), ...init,
    headers: {Origin: APP_ORIGIN, "Content-Type": "application/json", ...init.headers}});
  const handler = {GET: handlers.GET, POST: handlers.POST, PATCH: handlers.PATCH, DELETE: handlers.DELETE}[method] ?? handlers.GET;
  return handler(request, {params: Promise.resolve({workspaceId: WORKSPACE, resourcePath: parts})});
}

describe("workspace proxy", () => {
  it.each([undefined, "null", "https://foreign.test", "bad-url"])("rejects a foreign or missing mutation origin %s before minting", async (origin) => {
    const headers = new Headers({"Content-Type": "application/json"});
    if (origin !== undefined) headers.set("Origin", origin);
    const request = new Request(`${APP_ORIGIN}${BASE}/fleets`, {method: METHOD.post, body: "{}", headers});
    expect((await proxyWorkspace(request, {params: Promise.resolve({workspaceId: WORKSPACE, resourcePath: ["fleets"]})})).status).toBe(403);
    expect(credential).not.toHaveBeenCalled(); expect(upstream).not.toHaveBeenCalled();
  });
  it("uses the forwarded host and protocol for same-origin writes", async () => {
    upstream.mockResolvedValue(response({fleet: {id: FLEET_ID}}, 201));
    const result = await proxy(["fleets"], {method: METHOD.post, headers: {Host: "internal:3000", "X-Forwarded-Host": "app.example.test, edge.test", "X-Forwarded-Proto": "https, http"}});
    expect(result.status).toBe(201); expect(credential).toHaveBeenCalledTimes(1);
  });
  it("refuses an unlisted route or unsupported method before credential access", async () => {
    expect((await proxy(["secrets"], {method: METHOD.post})).status).toBe(404);
    expect((await proxy(["drafts"], {method: METHOD.post})).status).toBe(404);
    expect((await proxy(["fleets", FLEET_ID, "setup-context"])).status).toBe(404);
    expect((await proxy(["fleets"], {method: "PUT"})).status).toBe(404);
    expect(credential).not.toHaveBeenCalled();
  });
  it("refuses a non-JSON or oversized mutation before minting", async () => {
    expect((await proxy(["fleets"], {method: METHOD.post, headers: {"Content-Type": "text/plain"}})).status).toBe(415);
    expect((await proxy(["fleets"], {method: METHOD.post, headers: {"Content-Length": String(CHAT_BODY_LIMIT_BYTES + 1)}})).status).toBe(413);
    expect((await proxy(["fleets"], {method: METHOD.post, body: "x".repeat(CHAT_BODY_LIMIT_BYTES + 1)})).status).toBe(413);
    expect(credential).not.toHaveBeenCalled();
  });
  it("forwards a schedule sync retry with no body and no JSON media type", async () => {
    upstream.mockResolvedValue(response({schedule_id: SCHEDULE_ID, sync: "synced"}));
    const parts = ["fleets", FLEET_ID, "schedules", SCHEDULE_ID, "sync"];
    const request = new Request(`${APP_ORIGIN}${BASE}/${parts.join("/")}`, {method: METHOD.post, headers: {Origin: APP_ORIGIN}});
    const result = await handlers.POST(request, {params: Promise.resolve({workspaceId: WORKSPACE, resourcePath: parts})});
    expect(result.status).toBe(200);
    const forwarded = upstream.mock.calls[0]?.[1];
    expect(forwarded.body).toBeUndefined(); expect(forwarded.headers.get("Content-Type")).toBeNull();
  });
  it("forwards a turn's Stop as the PATCH it is, to the fleet's turn", async () => {
    upstream.mockResolvedValue(response({turn_id: TURN_ID, status: "cancelled"}));
    const body = JSON.stringify({status: "cancelled"});
    const request = new Request(`${APP_ORIGIN}${BASE}/fleets/${FLEET_ID}/turns/${TURN_ID}`, {method: METHOD.patch, body,
      headers: {Origin: APP_ORIGIN, "Content-Type": "application/json"}});
    const result = await stopTurnRoute(request, {params: Promise.resolve({workspaceId: WORKSPACE, fleetId: FLEET_ID, turnId: TURN_ID})});
    expect(result.status).toBe(200);
    const [url, init] = upstream.mock.calls[0] ?? [];
    expect(String(url)).toMatch(new RegExp(`/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}/turns/${TURN_ID}$`));
    expect(init.method).toBe(METHOD.patch); expect(new TextDecoder().decode(init.body)).toBe(body);
  });
  it("forwards a task's statement edit with its version to the fleet's task", async () => {
    upstream.mockResolvedValue(response({title: "task"}));
    const body = JSON.stringify({questions: [], expected_generation: 1});
    const request = new Request(`${APP_ORIGIN}${BASE}/fleets/${FLEET_ID}/durable_tasks/${TURN_ID}`, {method: METHOD.patch, body,
      headers: {Origin: APP_ORIGIN, "Content-Type": "application/json", "If-Match": '"task"'}});
    expect((await taskRoute(request, {params: Promise.resolve({workspaceId: WORKSPACE, fleetId: FLEET_ID, durableTaskId: TURN_ID})})).status).toBe(200);
    const [url, init] = upstream.mock.calls[0] ?? [];
    expect(String(url)).toMatch(new RegExp(`/fleets/${FLEET_ID}/durable_tasks/${TURN_ID}$`));
    expect(init.headers.get("If-Match")).toBe('"task"'); expect(new TextDecoder().decode(init.body)).toBe(body);
  });
  it("returns private unauthorized state without forwarding when the session expired", async () => {
    credential.mockResolvedValue(null);
    const result = await proxy(["models"]);
    expect(result.status).toBe(401); expect(result.headers.get("cache-control")).toBe("private, no-store"); expect(upstream).not.toHaveBeenCalled();
  });
  it("forwards exact bytes, the version header and only safe presentation headers", async () => {
    upstream.mockResolvedValue(response({detail: "Name taken", error_code: "UZ-AGT-006"}, 409,
      {ETag: '"current"', Location: "/safe", "Retry-After": "1", [PROXY_REQUEST_ID_HEADER]: REQUEST_ID, "Set-Cookie": "never-forward", Authorization: "never-forward"}));
    const result = await proxy(["fleets"], {method: METHOD.post, body: '{"message":"text"}',
      headers: {"Idempotency-Key": "same-operation", "If-Match": '"fleet"', "X-Not-Forwarded": "private"}});
    expect(result.status).toBe(409); expect(await result.json()).toEqual({detail: "Name taken", error_code: "UZ-AGT-006"});
    expect(result.headers.get("etag")).toBe('"current"'); expect(result.headers.get("set-cookie")).toBeNull(); expect(result.headers.get("authorization")).toBeNull();
    expect(result.headers.get(PROXY_REQUEST_ID_HEADER)).toBe(REQUEST_ID);
    const request = upstream.mock.calls[0]?.[1];
    expect(new TextDecoder().decode(request.body)).toBe('{"message":"text"}');
    expect(request.headers.get("Idempotency-Key")).toBe("same-operation"); expect(request.headers.get("If-Match")).toBe('"fleet"');
    expect(request.headers.get("Authorization")).toBe("Bearer synthetic-session"); expect(request.headers.get("X-Not-Forwarded")).toBeNull();
    expect(request.signal).toBeInstanceOf(AbortSignal);
  });
  it("whitelists list parameters and omits arbitrary upstream queries", async () => {
    upstream.mockResolvedValue(response({items: []}));
    await proxy(["models"], {}, "?purpose=chat&limit=10&starting_after=cursor&credential=secret");
    const target = new URL(upstream.mock.calls[0]?.[0]);
    expect(target.pathname).toBe(`/v1/workspaces/${WORKSPACE}/models`);
    expect(target.search).toBe("?limit=10&starting_after=cursor&purpose=chat");
    await proxy(["models"]);
    expect(new URL(upstream.mock.calls.at(-1)?.[0]).search).toBe("");
  });
  it("takes a name only on the fleet list, and pages only the memory list", async () => {
    upstream.mockResolvedValue(response({items: [], total: null, next_cursor: null}));
    await proxy(["fleets"], {}, "?name=agentbob&credential=secret");
    expect(new URL(upstream.mock.calls[0]?.[0]).search).toBe("?name=agentbob");
    await proxy(["fleets", FLEET_ID], {}, "?name=agentbob");
    expect(new URL(upstream.mock.calls[1]?.[0]).search).toBe("");
    await proxy(["fleets", FLEET_ID, "memories"], {}, "?limit=100&starting_after=key&name=x");
    expect(new URL(upstream.mock.calls[2]?.[0]).search).toBe("?limit=100&starting_after=key");
    await proxy(["fleets", FLEET_ID, "triggers"], {}, "?limit=100");
    expect(new URL(upstream.mock.calls[3]?.[0]).search).toBe("");
  });
  it("forwards a pinned memory's key as one encoded segment", async () => {
    upstream.mockResolvedValue(response(null, 204));
    const result = await proxy(["fleets", FLEET_ID, "memories", "evidence/links ✓"], {method: METHOD.delete});
    expect(result.status).toBe(204); expect(await result.text()).toBe("");
    expect(new URL(upstream.mock.calls[0]?.[0]).pathname).toBe(`/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}/memories/${encodeURIComponent("evidence/links ✓")}`);
  });
  it("strips content types outside the allowed set and keeps uncertain upstream failure distinct", async () => {
    upstream.mockResolvedValueOnce(new Response("unexpected", {headers: {"Content-Type": "text/html"}})).mockRejectedValueOnce(new Error("unconfirmed"));
    expect((await proxy(["fleets", FLEET_ID])).headers.get("content-type")).toBe("text/plain");
    const result = await proxy(["fleets"], {method: METHOD.post});
    expect(result.status).toBe(502); expect(await result.json()).toEqual({detail: PROXY_DETAIL.unconfirmed});
  });
});

describe("workspace route whitelist", () => {
  const allowed = [
    [["models"], METHOD.get], [["fleets"], METHOD.get], [["fleets"], METHOD.post],
    [["fleets", FLEET_ID], METHOD.get], [["fleets", FLEET_ID], METHOD.patch],
    [["fleets", FLEET_ID, "schedules"], METHOD.get], [["fleets", FLEET_ID, "triggers"], METHOD.get],
    [["fleets", FLEET_ID, "integration-grants"], METHOD.get], [["fleets", FLEET_ID, "memories"], METHOD.get],
    [["fleets", FLEET_ID, "turns", TURN_ID], METHOD.patch], [["fleets", FLEET_ID, "durable_tasks", TURN_ID], METHOD.patch],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID], METHOD.get], [["fleets", FLEET_ID, "schedules", SCHEDULE_ID], METHOD.patch],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID], METHOD.delete],
    [["fleets", FLEET_ID, "triggers", TRIGGER_ID], METHOD.delete], [["fleets", FLEET_ID, "integration-grants", GRANT_ID], METHOD.delete],
    [["fleets", FLEET_ID, "memories", "standing-guidance"], METHOD.delete], [["fleets", FLEET_ID, "memories", ".hidden"], METHOD.delete],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID, "sync"], METHOD.post],
  ] as const;
  it.each(allowed)("matches %s %s", (path, method) => {expect(matchWorkspaceRoute(WORKSPACE, path, method)).toBeDefined();});
  it("rejects a malformed workspace", () => {expect(matchWorkspaceRoute("..", ["models"], METHOD.get)).toBeUndefined();});
  it.each([
    [["models", FLEET_ID], METHOD.get], [["models"], METHOD.post], [["secrets"], METHOD.get],
    [["drafts"], METHOD.get], [["drafts"], METHOD.post], [["fleets"], METHOD.patch], [["fleets"], METHOD.delete],
    [["fleets", "invalid"], METHOD.get], [["fleets", FLEET_ID], METHOD.delete],
    [["fleets", FLEET_ID, "revisions"], METHOD.post], [["fleets", FLEET_ID, "setup-context"], METHOD.get],
    [["fleets", FLEET_ID, "triggers"], METHOD.post], [["fleets", FLEET_ID, "secrets"], METHOD.get],
    [["fleets", FLEET_ID, "memories", ".."], METHOD.delete], [["fleets", FLEET_ID, "memories", ""], METHOD.delete],
    [["fleets", FLEET_ID, "memories", "standing-guidance"], METHOD.get],
    [["fleets", FLEET_ID, "triggers", "invalid"], METHOD.delete], [["fleets", FLEET_ID, "triggers", TRIGGER_ID], METHOD.get],
    [["fleets", FLEET_ID, "integration-grants", GRANT_ID], METHOD.patch],
    [["fleets", FLEET_ID, "schedules", "invalid"], METHOD.get], [["fleets", FLEET_ID, "schedules", SCHEDULE_ID], METHOD.post],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID, "sync"], METHOD.get],
    [["fleets", FLEET_ID, "schedules", "invalid", "sync"], METHOD.post],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID, "other"], METHOD.post],
    [["fleets", FLEET_ID, "triggers", TRIGGER_ID, "sync"], METHOD.post],
    // Only Stop crosses this proxy for a turn: its read keeps the JSON read route.
    [["fleets", FLEET_ID, "turns", TURN_ID], METHOD.get], [["fleets", FLEET_ID, "turns", "invalid"], METHOD.patch],
    [["fleets", FLEET_ID, "durable_tasks", TURN_ID], METHOD.delete],
    [["fleets", FLEET_ID, "schedules", SCHEDULE_ID, "sync", "extra"], METHOD.post],
  ] as const)("rejects an unsupported method/path pair %s %s", (path, method) => {
    expect(matchWorkspaceRoute(WORKSPACE, path, method)).toBeUndefined();
  });
});
