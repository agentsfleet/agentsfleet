import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

describe("listTenantWorkspaces", () => {
  it("GET /v1/tenants/me/workspaces with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        items: [{ id: "ws_1", name: "alpha", created_at: 100 }],
        total: 1,
      }),
    });
    const { listTenantWorkspaces } = await import("./workspaces");
    const res = await listTenantWorkspaces("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/workspaces"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.items[0]?.id).toBe("ws_1");
    expect(res.total).toBe(1);
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ error: "unauthorized", code: "UZ-AUTH-001" }),
    });
    const { listTenantWorkspaces } = await import("./workspaces");
    await expect(listTenantWorkspaces("bad")).rejects.toBeInstanceOf(ApiError);
  });
});
