import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NANOS_PER_USD } from "@/lib/types";

// Pure API-client tests — these exercise lib/api/fleets + lib/api/tenant_billing
// against a stubbed fetch, with no React/component mocks. The component and route
// tests for this domain live in fleets-routes / fleets-components / fleets-install.
const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  fetchMock.mockReset();
});

describe("lib/api/fleets", () => {
  it("listFleets sends GET with bearer and parses the envelope", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "zom_1" }], total: 1, next_cursor: null }),
    });
    const mod = await import("../lib/api/fleets");
    const res = await mod.listFleets("ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("installFleet sends POST body and returns the created fleet", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ fleet_id: "zom_2", status: "active" }),
    });
    const mod = await import("../lib/api/fleets");
    const body = { platform_library_id: "github-pr-reviewer", name: "platform-ops" };
    const res = await mod.installFleet("ws_1", body, "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(body),
      }),
    );
    expect(res.fleet_id).toBe("zom_2");
  });

  it("installFleet surfaces API error status + code", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ detail: "name taken", error_code: "UZ-ZOM-002" }),
    });
    const mod = await import("../lib/api/fleets");
    await expect(
      mod.installFleet(
        "ws_1",
        { platform_library_id: "github-pr-reviewer", name: "dup" },
        "tkn",
      ),
    ).rejects.toMatchObject({ status: 409, code: "UZ-ZOM-002", message: "name taken" });
  });

  it("installFleet falls back to statusText when body is unparseable", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => {
        throw new Error("bad json");
      },
    });
    const mod = await import("../lib/api/fleets");
    await expect(
      mod.installFleet(
        "ws_1",
        { tenant_library_id: "01932d4e-7c10-7a3a-9f00-000000000001" },
        "tkn",
      ),
    ).rejects.toMatchObject({ status: 500, message: "Server Error" });
  });

  it("deleteFleet sends DELETE and returns void on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const mod = await import("../lib/api/fleets");
    const res = await mod.deleteFleet("ws_1", "zom_2", "tkn");
    expect(res).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("getFleet returns the fleet detail with its ETag", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ etag: '"fleet-v1"' }),
      json: async () => ({ id: "zom_2", name: "ops" }),
    });
    const mod = await import("../lib/api/fleets");
    const res = await mod.getFleet("ws_1", "zom_2", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2"),
      expect.objectContaining({ method: "GET" }),
    );
    expect(res).toEqual({ fleet: { id: "zom_2", name: "ops" }, etag: '"fleet-v1"' });
  });

  it("saveFleetSource sends If-Match and falls back to the response ETag", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: new Headers({ etag: '"fleet-v2"' }),
      json: async () => ({ config_revision: 4 }),
    });
    const mod = await import("../lib/api/fleets");
    const body = { trigger_markdown: "on: cron" };
    const res = await mod.saveFleetSource("ws_1", "zom_2", body, '"fleet-v1"', "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2"),
      expect.objectContaining({
        method: "PATCH",
        headers: expect.objectContaining({ "If-Match": '"fleet-v1"' }),
        body: JSON.stringify(body),
      }),
    );
    expect(res).toEqual({ etag: '"fleet-v2"', config_revision: 4 });
  });

});

describe("lib/api/memory", () => {
  it("listMemories sends GET without a query string when no limit is set", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [], total: 0, request_id: "req_1" }),
    });
    const mod = await import("../lib/api/memory");
    const res = await mod.listMemories("ws_1", "zom_2", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories"),
      expect.objectContaining({ method: "GET" }),
    );
    expect(String(fetchMock.mock.calls[0]?.[0] ?? "")).not.toContain("?limit=");
    expect(res.total).toBe(0);
  });

  it("listMemories includes the limit when provided", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ key: "style" }], total: 1, request_id: "req_2" }),
    });
    const mod = await import("../lib/api/memory");
    await mod.listMemories("ws_1", "zom_2", "tkn", { limit: 25 });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories?limit=25"),
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("forgetMemory path-encodes the memory key and returns void", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const mod = await import("../lib/api/memory");
    await expect(mod.forgetMemory("ws_1", "zom_2", "style/key", "tkn")).resolves.toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories/style%2Fkey"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("lib/api/tenant_billing", () => {
  it("getTenantBilling sends GET with bearer and returns the snapshot", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        balance_nanos: NANOS_PER_USD,
        updated_at: 1713700000000,
        is_exhausted: false,
        exhausted_at: null,
      }),
    });
    const mod = await import("../lib/api/tenant_billing");
    const res = await mod.getTenantBilling("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing"),
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.is_exhausted).toBe(false);
  });

  it("getTenantBilling throws with status + code on error", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      json: async () => ({ detail: "bad token", error_code: "UZ-AUTH-001" }),
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("bad")).rejects.toMatchObject({
      status: 401,
      code: "UZ-AUTH-001",
      message: "bad token",
    });
  });

  it("getTenantBilling falls back to statusText when body parse fails", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => {
        throw new Error("bad json");
      },
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("tok")).rejects.toMatchObject({
      status: 500,
      message: "Server Error",
    });
  });
});
