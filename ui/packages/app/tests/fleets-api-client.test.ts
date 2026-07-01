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
    const body = { platform_template_id: "github-pr-reviewer", name: "platform-ops" };
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
        { platform_template_id: "github-pr-reviewer", name: "dup" },
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
        { tenant_template_id: "01932d4e-7c10-7a3a-9f00-000000000001" },
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

  it("webhookUrlFor composes the deterministic webhook URL", async () => {
    const mod = await import("../lib/api/fleets");
    expect(mod.webhookUrlFor("zom_abc")).toBe(
      "https://api-dev.agentsfleet.net/v1/webhooks/zom_abc",
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
