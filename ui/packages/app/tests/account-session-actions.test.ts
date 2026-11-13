// @vitest-environment node
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { listAccountSessionsAction, revokeAccountSessionAction } from "@/lib/auth/account-actions";

const mocks = vi.hoisted(() => {
  vi.stubEnv("CLERK_SECRET_KEY", ["sk", "test", "localfixture"].join("_"));
  return {auth: vi.fn(), fetch: vi.fn()};
});
vi.mock("server-only", () => ({}));
vi.mock("@clerk/nextjs/server", async importOriginal => {
  const actual = await importOriginal<typeof import("@clerk/nextjs/server")>();
  return {...actual, auth: mocks.auth};
});

beforeEach(() => {
  mocks.auth.mockReset().mockResolvedValue({userId: "user_owner", sessionId: "sess_current", has: () => true});
  mocks.fetch.mockReset();
  vi.stubGlobal("fetch", mocks.fetch);
});
afterEach(() => {vi.unstubAllGlobals(); vi.unstubAllEnvs(); vi.restoreAllMocks();});

it("reads fresh, bounded, current-person session pages and maps only app-owned fields", async () => {
  mocks.fetch.mockResolvedValue(Response.json({data: [session("sess_current")], total_count: 51}));
  const result = await listAccountSessionsAction("user_owner");
  expect(result).toMatchObject({ok: true, data: {nextOffset: 1, sessions: [{id: "sess_current", current: true,
    browser: "Chrome", device: "macOS", location: "Chennai, IN", lastActiveAt: "2026-11-11T10:00:00.000Z"}]}});
  const url = new URL(String(mocks.fetch.mock.calls[0]?.[0]));
  expect(url.searchParams.get("user_id")).toBe("user_owner");
  expect(url.searchParams.get("status")).toBe("active");
  expect(url.searchParams.get("limit")).toBe("50");
  expect(url.searchParams.get("paginated")).toBe("true");
  expect(mocks.fetch.mock.calls[0]?.[1]?.headers["Clerk-API-Version"]).toBe("2026-05-12");
  expect(JSON.stringify(result)).not.toContain("private_ip");
  mocks.fetch.mockResolvedValue(Response.json({data: [], total_count: 0}));
  expect(await listAccountSessionsAction("user_owner")).toEqual({ok: true, data: {sessions: [], nextOffset: null}});
  expect(mocks.fetch).toHaveBeenCalledTimes(2);
});

it.each([null, {userId: null, sessionId: null}])("refuses signed-out session reads and writes without provider calls", async identity => {
  mocks.auth.mockResolvedValue(identity ?? {});
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 401});
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 401});
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it("rejects stale owners, traversal identifiers and invalid paging before calling the provider", async () => {
  expect(await listAccountSessionsAction("user_else")).toMatchObject({ok: false, status: 409});
  expect(await listAccountSessionsAction("user_owner", -1)).toMatchObject({ok: false, status: 400});
  expect(await revokeAccountSessionAction("user_owner", "../users")).toMatchObject({ok: false, status: 400});
  expect(await revokeAccountSessionAction("user_else", "sess_other")).toMatchObject({ok: false, status: 409});
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it("requires recent verification and refuses revoking the current session before reading targets", async () => {
  expect(await revokeAccountSessionAction("user_owner", "sess_current")).toMatchObject({ok: false, status: 409});
  const has = vi.fn().mockReturnValue(false);
  mocks.auth.mockResolvedValue({userId: "user_owner", sessionId: "sess_current", has});
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({clerk_error: {metadata: {reverification: "strict"}}});
  expect(has).toHaveBeenCalledExactlyOnceWith({reverification: "strict"});
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it("never revokes another person's session", async () => {
  mocks.fetch.mockResolvedValue(Response.json(session("sess_other", "user_else")));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 404});
  expect(mocks.fetch).toHaveBeenCalledTimes(1);
});

it("refuses personal session operations while impersonating another person", async () => {
  mocks.auth.mockResolvedValue({userId: "user_owner", sessionId: "sess_current", actor: {sub: "operator"}, has: () => true});
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 403});
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 403});
  expect(mocks.fetch).not.toHaveBeenCalled();
});

it("confirms revocation only after the provider records a terminal result", async () => {
  mocks.fetch.mockResolvedValueOnce(Response.json(session("sess_other")))
    .mockResolvedValueOnce(Response.json({...session("sess_other"), status: "revoked"}));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toEqual({ok: true, data: {id: "sess_other"}});
  expect(String(mocks.fetch.mock.calls[1]?.[0])).toContain("/sessions/sess_other/revoke");
  expect(mocks.fetch.mock.calls[1]?.[1]?.method).toBe("POST");
});

it("reconciles an already-ended session without repeating its revocation", async () => {
  mocks.fetch.mockResolvedValue(Response.json({...session("sess_other"), status: "revoked"}));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toEqual({ok: true, data: {id: "sess_other"}});
  expect(mocks.fetch).toHaveBeenCalledTimes(1);
});

it("confirms a committed sign-out after a lost response without repeating its mutation", async () => {
  let status = "active";
  mocks.fetch.mockImplementation(async (_url, init: RequestInit) => {
    if (init.method === "POST") {
      status = "revoked";
      throw new Error("The response connection was lost");
    }
    return Response.json({...session("sess_other"), status});
  });
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 503,
    error: expect.stringContaining("could not be confirmed")});
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toEqual({ok: true, data: {id: "sess_other"}});
  expect(mocks.fetch.mock.calls.filter(([, init]) => init.method === "POST")).toHaveLength(1);
});

it("does not claim success for an unchanged, foreign or malformed revoke response", async () => {
  mocks.fetch.mockResolvedValueOnce(Response.json(session("sess_other"))).mockResolvedValueOnce(Response.json(session("sess_other")));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 503, error: expect.stringContaining("could not be confirmed")});
});

it("keeps failed or mixed-owner reads unavailable instead of displaying empty or foreign sessions", async () => {
  mocks.fetch.mockResolvedValue(Response.json({data: [session("sess_foreign", "user_else")], total_count: 1}));
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 503});
  mocks.fetch.mockResolvedValue(Response.json({errors: [{code: "access_denied", message: "private provider detail"}]}, {status: 403}));
  const result = await listAccountSessionsAction("user_owner");
  expect(result).toMatchObject({ok: false, status: 503});
  expect(JSON.stringify(result)).not.toContain("private provider detail");
});

it("rejects an incomplete session page rather than creating an endless paging loop", async () => {
  mocks.fetch.mockResolvedValue(Response.json({data: [], total_count: 2}));
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 503});
});

it("keeps absent activity times unknown and refuses coercible nonnumeric values", async () => {
  mocks.fetch.mockResolvedValue(Response.json({data: [{...session("sess_other"), last_active_at: null}], total_count: 1}));
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: true, data: {sessions: [{lastActiveAt: null}]}});
  mocks.fetch.mockResolvedValue(Response.json({data: [{...session("sess_other"), last_active_at: false}], total_count: 1}));
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 503});
});

it("cancels an oversized declared response before attempting to parse it", async () => {
  const cancel = vi.fn();
  mocks.fetch.mockResolvedValue(new Response(new ReadableStream({cancel}), {headers: {"content-length": String(2 * 1024 * 1024)}}));
  expect(await listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 503});
  expect(cancel).toHaveBeenCalledOnce();
});

it("aborts a stalled provider connection at the operation deadline", async () => {
  vi.useFakeTimers();
  const controller = new AbortController();
  vi.spyOn(AbortSignal, "timeout").mockImplementation(ms => {setTimeout(() => controller.abort(), ms); return controller.signal;});
  mocks.fetch.mockImplementation((_url, init: RequestInit) => new Promise((_resolve, reject) => {
    init.signal?.addEventListener("abort", () => reject(new Error("Stopped")), {once: true});
  }));
  const reading = listAccountSessionsAction("user_owner");
  await vi.advanceTimersByTimeAsync(10_000);
  expect(await reading).toMatchObject({ok: false, status: 503});
  expect(controller.signal.aborted).toBe(true);
});

it("cancels a stalled response body without issuing a subsequent revocation", async () => {
  vi.useFakeTimers();
  const controller = new AbortController();
  const cancel = vi.fn();
  vi.spyOn(AbortSignal, "timeout").mockImplementation(ms => {setTimeout(() => controller.abort(), ms); return controller.signal;});
  mocks.fetch.mockResolvedValue(new Response(new ReadableStream({cancel})));
  const revoking = revokeAccountSessionAction("user_owner", "sess_other");
  await vi.advanceTimersByTimeAsync(10_000);
  expect(await revoking).toMatchObject({ok: false, status: 503});
  expect(cancel).toHaveBeenCalledOnce();
  expect(mocks.fetch).toHaveBeenCalledTimes(1);
});

it("keeps unknown device and activity fields unknown and omits ended sessions", async () => {
  mocks.fetch.mockResolvedValue(Response.json({data: [{...session("sess_other"), latest_activity: null, last_active_at: 1e20},
    {...session("sess_ended"), status: "ended"}], total_count: 2}));
  expect(await listAccountSessionsAction("user_owner", 0)).toMatchObject({ok: true, data: {sessions: [{id: "sess_other", browser: "Unknown browser",
    device: "Unknown device", location: null, mobile: false, lastActiveAt: null}], nextOffset: null}});
});

it("refuses unknown target states and mismatched response identities before a mutation", async () => {
  mocks.fetch.mockResolvedValueOnce(Response.json({...session("sess_other"), status: "pending"}));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 409});
  mocks.fetch.mockResolvedValueOnce(Response.json(session("sess_different")));
  expect(await revokeAccountSessionAction("user_owner", "sess_other")).toMatchObject({ok: false, status: 404});
  expect(mocks.fetch.mock.calls.every(([, init]) => init.method === "GET")).toBe(true);
});

it.each([503, 200])("keeps cancellation cleanup failure private for a refused response (%s)", async status => {
  mocks.fetch.mockResolvedValue(new Response(new ReadableStream({start(controller) {controller.error(new Error("private stream detail"));}}),
    {status, headers: {"content-length": String(2 * 1024 * 1024)}}));
  const result = await listAccountSessionsAction("user_owner");
  expect(result).toMatchObject({ok: false, status: 503});
  expect(JSON.stringify(result)).not.toContain("private stream detail");
});

it("reports an unavailable provider key without reading a target", async () => {
  vi.resetModules(); vi.stubEnv("CLERK_SECRET_KEY", "");
  const actions = await import("@/lib/auth/account-actions");
  expect(await actions.listAccountSessionsAction("user_owner")).toMatchObject({ok: false, status: 503});
  expect(mocks.fetch).not.toHaveBeenCalled();
});

function session(id: string, userId = "user_owner") {
  return {object: "session", id, client_id: "client_browser", user_id: userId, status: "active", last_active_at: Date.parse("2026-11-11T10:00:00Z"),
    expire_at: Date.parse("2026-12-11T10:00:00Z"), abandon_at: 0, created_at: 0, updated_at: 0,
    latest_activity: {id: "activity", browser_name: "Chrome", device_type: "macOS", city: "Chennai", country: "IN", ip_address: "private_ip", is_mobile: false}};
}
