import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../lib/api/errors";

const fetchMock = vi.fn();
const cookieGet = vi.fn();
const authMock = vi.fn();

vi.stubGlobal("fetch", fetchMock);

vi.mock("next/headers", () => ({
  cookies: vi.fn(async () => ({ get: cookieGet })),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: authMock,
}));

beforeEach(() => {
  vi.clearAllMocks();
  authMock.mockResolvedValue({ sessionClaims: {} });
});

afterEach(() => {
  fetchMock.mockReset();
  cookieGet.mockReset();
  authMock.mockReset();
});

// Builds a fetch double that resolves GET /v1/tenants/me/workspaces to the
// given ids (creation-order ascending, so items[0] is the deterministic
// fallback). Used only by the list-fallback path; the cookie/claim paths must
// never trigger it (that is the round-trip the resolver exists to avoid).
function mockListTenantWorkspaces(ids: string[]) {
  fetchMock.mockResolvedValue({
    ok: true,
    status: 200,
    json: async () => ({
      items: ids.map((id, i) => ({ id, name: `ws-${i}`, created_at: i })),
      total: ids.length,
    }),
  });
}

function apiError(status: number) {
  return new ApiError("denied", status, "UZ-TEST", "req_test");
}

// ── §1 — resolveActiveWorkspaceId: cheap hint resolution, no validation ──────
describe("resolveActiveWorkspaceId", () => {
  // Scenario: cookie present (the common hot path).
  it("1.1 prefers the cookie and issues NO list fetch", async () => {
    cookieGet.mockReturnValue({ value: "ws_cookie" });
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_11");
    expect(result).toEqual({ id: "ws_cookie", source: "cookie" });
    expect(cookieGet).toHaveBeenCalledWith("active_workspace_id");
    expect(fetchMock).not.toHaveBeenCalled(); // invariant 3: no round-trip on the hint path
  });

  // Scenario A (no cookie) → claim hint.
  it("1.2 falls to the JWT claim when the cookie is absent, NO list fetch", async () => {
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { workspace_id: "ws_claim" } } });
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_12");
    expect(result).toEqual({ id: "ws_claim", source: "claim" });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  // Scenario A (no hint at all) → authoritative list, first item.
  it("1.3 falls to the list (first item) when neither cookie nor claim resolve", async () => {
    cookieGet.mockReturnValue(undefined);
    mockListTenantWorkspaces(["ws_first", "ws_second"]);
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_13");
    expect(result).toEqual({ id: "ws_first", source: "list" });
  });

  // Scenario E (no hint, empty tenant) → null → caller renders empty state.
  it("1.4 returns null when no hint and the list is empty", async () => {
    cookieGet.mockReturnValue(undefined);
    mockListTenantWorkspaces([]);
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    expect(await resolveActiveWorkspaceId("tok_14")).toBeNull();
  });

  // Graceful degradation: the list endpoint is down on the fallback path.
  it("1.5 returns null gracefully when the list fetch rejects", async () => {
    cookieGet.mockReturnValue(undefined);
    fetchMock.mockRejectedValue(new Error("network down"));
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    expect(await resolveActiveWorkspaceId("tok_15")).toBeNull();
  });

  // The resolver does NOT validate the cookie against the list — that is the
  // round-trip we removed. A foreign/stale cookie is returned as-is; the
  // backend + withWorkspaceScope handle rejection (scenarios B/C/D).
  it("1.6 returns a foreign cookie id WITHOUT validating it (no fetch)", async () => {
    cookieGet.mockReturnValue({ value: "ws_foreign" });
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_16");
    expect(result).toEqual({ id: "ws_foreign", source: "cookie" });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  // Claim malformed (non-string) → ignored, falls through to the list.
  it("1.7 ignores a non-string claim and falls to the list", async () => {
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { workspace_id: 123 } } });
    mockListTenantWorkspaces(["ws_first"]);
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_17");
    expect(result).toEqual({ id: "ws_first", source: "list" });
  });

  // auth() throws inside the claim read → swallowed, falls to the list.
  it("1.8 falls to the list when the auth provider throws", async () => {
    cookieGet.mockReturnValue(undefined);
    authMock.mockRejectedValueOnce(new Error("auth provider down"));
    mockListTenantWorkspaces(["ws_first"]);
    const { resolveActiveWorkspaceId } = await import("../lib/workspace");
    const result = await resolveActiveWorkspaceId("tok_18");
    expect(result).toEqual({ id: "ws_first", source: "list" });
  });
});

// ── §2 — withWorkspaceScope: stale-hint recovery ────────────────────────────
describe("withWorkspaceScope", () => {
  // Scenario B/C/D: stale hint rejected by the backend → re-resolve + retry.
  it("2.1 re-resolves via the list and retries once on a forbidden hint", async () => {
    cookieGet.mockReturnValue({ value: "ws_stale" });
    mockListTenantWorkspaces(["ws_real"]);
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi
      .fn<(id: string) => Promise<string>>()
      .mockRejectedValueOnce(apiError(403)) // first call with the stale hint
      .mockResolvedValueOnce("DATA"); // retry with the list id
    const result = await withWorkspaceScope("tok_21", fn);
    expect(result).toBe("DATA");
    expect(fn).toHaveBeenCalledTimes(2);
    expect(fn).toHaveBeenNthCalledWith(1, "ws_stale");
    expect(fn).toHaveBeenNthCalledWith(2, "ws_real");
  });

  // Invariant 1: a list-derived id never retries — its rejection is real.
  it("2.2 does NOT retry when a list-derived id is rejected", async () => {
    cookieGet.mockReturnValue(undefined); // no hint → source "list"
    mockListTenantWorkspaces(["ws_only"]);
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>().mockRejectedValue(apiError(403));
    await expect(withWorkspaceScope("tok_22", fn)).rejects.toBeInstanceOf(ApiError);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  // Scenario E: stale hint + tenant now owns zero workspaces → null, NOT a throw.
  it("2.4 returns null (no throw) when the hint is rejected and the list is empty", async () => {
    cookieGet.mockReturnValue({ value: "ws_ghost" });
    mockListTenantWorkspaces([]);
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>().mockRejectedValue(apiError(404));
    const result = await withWorkspaceScope("tok_24", fn);
    expect(result).toBeNull();
    expect(fn).toHaveBeenCalledTimes(1);
  });

  // No workspace at all (no hint, empty list) → null before fn ever runs.
  it("returns null without calling fn when the tenant owns no workspace", async () => {
    cookieGet.mockReturnValue(undefined);
    mockListTenantWorkspaces([]);
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>();
    expect(await withWorkspaceScope("tok_none", fn)).toBeNull();
    expect(fn).not.toHaveBeenCalled();
  });

  // Happy path: hint resolves, fn succeeds first try, no list fetch.
  it("runs fn once and issues no list fetch on the happy cookie path", async () => {
    cookieGet.mockReturnValue({ value: "ws_ok" });
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>().mockResolvedValue("OK");
    expect(await withWorkspaceScope("tok_ok", fn)).toBe("OK");
    expect(fn).toHaveBeenCalledExactlyOnceWith("ws_ok");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  // Non-workspace error (e.g. 500) never triggers a re-resolve — it propagates.
  it("propagates a non-workspace error without re-resolving", async () => {
    cookieGet.mockReturnValue({ value: "ws_ok" });
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>().mockRejectedValue(apiError(500));
    await expect(withWorkspaceScope("tok_500", fn)).rejects.toBeInstanceOf(ApiError);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  // Re-resolve yields the SAME id as the rejected hint → real error, rethrow
  // (guards against an infinite hint→list→hint loop).
  it("rethrows when re-resolution yields the same id (no retry loop)", async () => {
    cookieGet.mockReturnValue({ value: "ws_same" });
    mockListTenantWorkspaces(["ws_same"]);
    const { withWorkspaceScope } = await import("../lib/workspace");
    const fn = vi.fn<(id: string) => Promise<string>>().mockRejectedValue(apiError(403));
    await expect(withWorkspaceScope("tok_same", fn)).rejects.toBeInstanceOf(ApiError);
    expect(fn).toHaveBeenCalledTimes(1);
  });
});

// ── orFallback — degrade ordinary failures, re-throw workspace rejections ────
describe("orFallback", () => {
  it("returns the fallback for a non-workspace error", async () => {
    const { orFallback } = await import("../lib/workspace");
    const handler = orFallback({ items: [] });
    expect(handler(new Error("boom"))).toEqual({ items: [] });
  });

  it("re-throws a workspace rejection so the scope wrapper can retry", async () => {
    const { orFallback } = await import("../lib/workspace");
    const handler = orFallback({ items: [] });
    expect(() => handler(apiError(403))).toThrow(ApiError);
    expect(() => handler(apiError(404))).toThrow(ApiError);
  });

  it("returns the fallback for a non-403/404 ApiError (e.g. 500)", async () => {
    const { orFallback } = await import("../lib/workspace");
    const handler = orFallback({ items: [] });
    expect(handler(apiError(500))).toEqual({ items: [] });
  });
});
