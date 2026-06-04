import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

describe("lib/workspace resolveActiveWorkspace", () => {
  function mockListTenantWorkspaces(ids: string[]) {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        items: ids.map((id, i) => ({
          id,
          name: `ws-${i}`,
          created_at: i,
        })),
        total: ids.length,
      }),
    });
  }

  it("returns null when the tenant has no workspaces", async () => {
    mockListTenantWorkspaces([]);
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    expect(await resolveActiveWorkspace("tok")).toBeNull();
    expect(cookieGet).not.toHaveBeenCalled();
  });

  it("prefers the cookie-named workspace when it matches one the tenant owns", async () => {
    mockListTenantWorkspaces(["ws_1", "ws_2", "ws_3"]);
    cookieGet.mockReturnValue({ value: "ws_2" });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_2");
    expect(cookieGet).toHaveBeenCalledWith("active_workspace_id");
  });

  it("falls back to the JWT workspace_id claim when the cookie is absent", async () => {
    mockListTenantWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { workspace_id: "ws_2" } } });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_2");
  });

  it("falls back to the first workspace when neither cookie nor claim resolve", async () => {
    mockListTenantWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue(undefined);
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("falls back to the first workspace when the cookie names a foreign id", async () => {
    mockListTenantWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue({ value: "ws_not_ours" });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("returns null gracefully when listTenantWorkspaces rejects", async () => {
    fetchMock.mockRejectedValue(new Error("network down"));
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    expect(await resolveActiveWorkspace("tok")).toBeNull();
  });

  it("falls back to first workspace when readWorkspaceClaim throws (catch branch)", async () => {
    // auth() throws inside readWorkspaceClaim — the catch must swallow the
    // error and return null so resolveActiveWorkspace can still use items[0].
    mockListTenantWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue(undefined);
    authMock.mockRejectedValueOnce(new Error("auth provider down"));
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("falls back to first workspace when metadata has no workspace_id key", async () => {
    // sessionClaims.metadata is present but has no workspace_id field →
    // readWorkspaceClaim returns null → falls through to items[0].
    mockListTenantWorkspaces(["ws_1"]);
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: {} } });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("falls back to first workspace when workspace_id claim is a non-string value", async () => {
    // workspace_id present but not a string (e.g. numeric drift) →
    // the typeof guard in readWorkspaceClaim returns null → falls through to items[0].
    mockListTenantWorkspaces(["ws_1"]);
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { workspace_id: 123 } } });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("falls back to first workspace when the claim names a workspace the tenant no longer owns", async () => {
    // Cookie absent, but the JWT claim resolves a *real string* id that no
    // longer matches any owned workspace (stale primary after a delete/move).
    // readWorkspaceClaim returns the string, items.find misses → the
    // `if (match)` else-arm falls through to items[0]. Distinct from the
    // null-claim cases above, which never reach items.find at all.
    mockListTenantWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue(undefined);
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { workspace_id: "ws_stale" } } });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });
});
