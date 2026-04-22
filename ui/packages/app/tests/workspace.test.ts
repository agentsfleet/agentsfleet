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
          repo_url: null,
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
});
