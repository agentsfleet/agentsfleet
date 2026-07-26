import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

const TOKEN = "tok";
const TENANT_ID = "018f0000-0000-7000-8000-000000000001";
const OTHER_TENANT_ID = "018f0000-0000-7000-8000-000000000002";
const NEXT_CURSOR = "01900000-0000-7000-8000-000000000001";
const okResponse = (body: unknown) => ({
  ok: true,
  status: 200,
  json: async () => body,
});

afterEach(() => fetchMock.mockReset());

describe("listTenantWorkspaces", () => {
  it("GET /v1/tenants/me/workspaces with bearer, returns envelope", async () => {
    fetchMock.mockResolvedValue(
      okResponse({
        items: [{ id: "ws_1", name: "alpha", created_at: 100 }],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      }),
    );
    const { listTenantWorkspaces } = await import("./workspaces");
    const res = await listTenantWorkspaces(TOKEN);
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/workspaces"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.items[0]?.id).toBe("ws_1");
    expect(res.tenant_id).toBe(TENANT_ID);
    expect(res.total).toBe(1);
  });

  it("rejects a response without an authoritative tenant", async () => {
    fetchMock.mockResolvedValue(
      okResponse({ items: [], total: null, next_cursor: null }),
    );
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace response omitted tenant_id",
    );
  });

  it("rejects a null response before reading pagination fields", async () => {
    fetchMock.mockResolvedValue(okResponse(null));
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace response is invalid",
    );
  });

  it("rejects a response without an items array", async () => {
    fetchMock.mockResolvedValue(
      okResponse({
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      }),
    );
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace response omitted items",
    );
  });

  it("rejects a response without a pagination cursor", async () => {
    fetchMock.mockResolvedValue(
      okResponse({ items: [], tenant_id: TENANT_ID, total: null }),
    );
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace response omitted next_cursor",
    );
  });

  it("rejects a tenant change between cursor pages", async () => {
    fetchMock
      .mockResolvedValueOnce(
        okResponse({
          items: [],
          tenant_id: TENANT_ID,
          total: null,
          next_cursor: NEXT_CURSOR,
        }),
      )
      .mockResolvedValueOnce(
        okResponse({
          items: [],
          tenant_id: OTHER_TENANT_ID,
          total: null,
          next_cursor: null,
        }),
      );
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace pagination changed tenant",
    );
  });

  it("rejects a non-string cursor", async () => {
    fetchMock.mockResolvedValue(
      okResponse({
        items: [],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: 42,
      }),
    );
    const { listTenantWorkspaces } = await import("./workspaces");

    await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(
      "workspace pagination returned an invalid cursor",
    );
  });

  it("rejects malformed items and non-null page totals", async () => {
    const malformed = [
      {
        items: [null],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      },
      {
        items: [{ id: "", name: "bad", created_at: 1 }],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      },
      {
        items: [{ id: "ws_1", name: 42, created_at: 1 }],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      },
      {
        items: [{ id: "ws_1", name: "bad", created_at: Number.NaN }],
        tenant_id: TENANT_ID,
        total: null,
        next_cursor: null,
      },
      {
        items: [],
        tenant_id: TENANT_ID,
        total: 0,
        next_cursor: null,
      },
    ];
    const { listTenantWorkspaces } = await import("./workspaces");

    for (const page of malformed) {
      fetchMock.mockResolvedValueOnce(okResponse(page));
      await expect(listTenantWorkspaces(TOKEN)).rejects.toThrow(/invalid/);
    }
  });

  it("throws ApiError on 401", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      json: async () => ({ detail: "unauthorized", error_code: "UZ-AUTH-001" }),
    });
    const { listTenantWorkspaces } = await import("./workspaces");
    await expect(listTenantWorkspaces("bad")).rejects.toBeInstanceOf(ApiError);
  });
});
